import 'dart:convert';

import '../bridge/mpc_engine.dart';
import '../dto/mpc_dtos.dart';
import '../transport/mpc_transport.dart';
import 'mpc_exceptions.dart';

/// JSON-RPC 2.0 method names for MPC protocol operations.
enum MpcMethod {
  keygenStart('keygen_start'),
  keygenContinue('keygen_continue'),
  recoveryStart('recovery_start'),
  recoveryContinue('recovery_continue'),
  signStart('sign_start'),
  signContinue('sign_continue'),
  exportKey('export_key');

  const MpcMethod(this.method);

  /// The JSON-RPC method name.
  final String method;
}

/// High-level MPC orchestration API for host applications.
///
/// Communicates with the server via JSON-RPC 2.0 protocol.
/// Drives keygen/recovery/sign round-trips by coordinating between
/// [MpcEngine] (Rust FFI) and [MpcTransport] (host-injected network).
class MpcClient {
  final MpcEngine _engine;
  final MpcTransport _transport;
  int _requestId = 0;

  MpcClient({required MpcEngine engine, required MpcTransport transport})
      : _engine = engine,
        _transport = transport;

  /// Execute full keygen protocol.
  Future<KeygenResult> keygen() async {
    final initData = await _rpcCall(MpcMethod.keygenStart, {});
    final sessionId = initData['sessionId'] as String;

    final round1 = await _engine.keygenStart(
      sessionId,
      jsonEncode(initData['serverPayload']),
    );
    _checkProtocolError(round1);

    var currentResult = round1;
    while (currentResult.isContinue) {
      final serverData = await _rpcCall(
        MpcMethod.keygenContinue,
        {
          'sessionId': sessionId,
          'round': currentResult.round,
          'clientPayload': currentResult.clientPayload,
        },
      );

      if (serverData['status'] == 'completed') {
        return KeygenResult.fromJson(serverData);
      }

      currentResult = await _engine.keygenContinue(
        sessionId,
        jsonEncode(serverData['serverPayload']),
      );
      _checkProtocolError(currentResult);
    }

    if (currentResult.isCompleted && currentResult.clientPayload != null) {
      final payload =
          jsonDecode(currentResult.clientPayload!) as Map<String, dynamic>;
      return KeygenResult.fromJson(_snakeToCamelKeys(payload));
    }

    throw MpcProtocolException(
        'Unexpected keygen state: ${currentResult.status}');
  }

  /// Execute full recovery protocol.
  Future<RecoveryResult> recover({
    required String mpcKeyId,
    required String encryptedBackupShare,
    required String userBackupSecret,
    required int currentRotationVersion,
    String? newUserBackupSecret,
  }) async {
    final backupShare = await _engine.decryptBackupShare(
      encryptedBackupShare,
      userBackupSecret,
    );

    final initData = await _rpcCall(
      MpcMethod.recoveryStart,
      {'mpcKeyId': mpcKeyId},
    );
    final sessionId = initData['sessionId'] as String;

    final round1 = await _engine.recoverStart(
      sessionId,
      backupShare,
      jsonEncode(initData['serverPayload']),
      currentRotationVersion,
    );
    _checkProtocolError(round1);

    var currentResult = round1;
    while (currentResult.isContinue) {
      final serverData = await _rpcCall(
        MpcMethod.recoveryContinue,
        {
          'sessionId': sessionId,
          'round': currentResult.round,
          'clientPayload': currentResult.clientPayload,
        },
      );

      if (serverData['status'] == 'completed') {
        return RecoveryResult.fromJson(serverData);
      }

      currentResult = await _engine.recoverContinue(
        sessionId,
        jsonEncode(serverData['serverPayload']),
      );
      _checkProtocolError(currentResult);
    }

    if (currentResult.isCompleted && currentResult.clientPayload != null) {
      final payload =
          jsonDecode(currentResult.clientPayload!) as Map<String, dynamic>;
      var result = RecoveryResult.fromJson(_snakeToCamelKeys(payload));

      if (newUserBackupSecret != null) {
        final envelope = await _engine.deriveBackupEnvelope(
          result.localEncryptedShare,
          newUserBackupSecret,
          DateTime.now().toUtc().toIso8601String(),
        );
        result = RecoveryResult(
          mpcKeyId: result.mpcKeyId,
          address: result.address,
          publicKey: result.publicKey,
          rotationVersion: result.rotationVersion,
          localEncryptedShare: result.localEncryptedShare,
          encryptedBackupShare: jsonEncode({
            'version': envelope.version,
            'algorithm': envelope.algorithm,
            'created_at': envelope.createdAt,
            'payload': envelope.payload,
          }),
        );
      }

      return result;
    }

    throw MpcProtocolException(
        'Unexpected recovery state: ${currentResult.status}');
  }

  /// Execute full sign protocol.
  Future<SignResult> sign({
    required String mpcKeyId,
    required String messageHash,
    required String localEncryptedShare,
  }) async {
    final initData = await _rpcCall(
      MpcMethod.signStart,
      {'mpcKeyId': mpcKeyId, 'messageHash': messageHash},
    );
    final sessionId = initData['sessionId'] as String;

    final round1 = await _engine.signStart(
      sessionId,
      localEncryptedShare,
      messageHash,
      jsonEncode(initData['serverPayload']),
    );
    _checkProtocolError(round1);

    var currentResult = round1;
    while (currentResult.isContinue) {
      final serverData = await _rpcCall(
        MpcMethod.signContinue,
        {
          'sessionId': sessionId,
          'round': currentResult.round,
          'clientPayload': currentResult.clientPayload,
        },
      );

      if (serverData['status'] == 'completed') {
        return SignResult.fromJson(serverData);
      }

      currentResult = await _engine.signContinue(
        sessionId,
        jsonEncode(serverData['serverPayload'] ?? {}),
      );
      _checkProtocolError(currentResult);
    }

    if (currentResult.isCompleted && currentResult.clientPayload != null) {
      final payload =
          jsonDecode(currentResult.clientPayload!) as Map<String, dynamic>;
      return SignResult.fromJson(payload);
    }

    throw MpcProtocolException(
        'Unexpected sign state: ${currentResult.status}');
  }

  /// Export MPC wallet to a standard wallet by reconstructing the full private key.
  Future<ExportResult> exportPrivateKey({
    required String mpcKeyId,
    required String localEncryptedShare,
  }) async {
    final serverData = await _rpcCall(
      MpcMethod.exportKey,
      {'mpcKeyId': mpcKeyId},
    );
    final serverSharePrivate = jsonEncode(serverData['serverSharePrivate']);

    final resultJson = await _engine.exportPrivateKey(
      localEncryptedShare,
      serverSharePrivate,
    );
    final payload = jsonDecode(resultJson) as Map<String, dynamic>;
    return ExportResult.fromJson(_snakeToCamelKeys(payload));
  }

  // ── JSON-RPC 2.0 helpers ──────────────────────────────────────

  /// Send a JSON-RPC 2.0 request and return the `result` field.
  ///
  /// Throws [MpcRpcException] if the server returns an error object.
  /// Throws [MpcTransportException] if the transport layer fails.
  Future<Map<String, dynamic>> _rpcCall(
    MpcMethod method,
    Map<String, dynamic> params,
  ) async {
    final id = ++_requestId;
    final request = jsonEncode({
      'jsonrpc': '2.0',
      'method': method.method,
      'params': params,
      'id': id,
    });

    final String rawResponse;
    try {
      rawResponse = await _transport.send(request);
    } catch (e) {
      throw MpcTransportException(
        'Transport failed: $e',
        method: method.method,
        cause: e,
      );
    }

    final Map<String, dynamic> response;
    try {
      response = jsonDecode(rawResponse) as Map<String, dynamic>;
    } catch (e) {
      throw MpcProtocolException('Invalid JSON-RPC response: $e');
    }

    // Check for JSON-RPC error
    if (response.containsKey('error') && response['error'] != null) {
      final error = response['error'] as Map<String, dynamic>;
      throw MpcRpcException(
        code: error['code'] as int,
        message: error['message'] as String,
        data: error['data'],
      );
    }

    // Extract result
    final result = response['result'];
    if (result == null) {
      throw MpcProtocolException(
        'JSON-RPC response missing "result" field',
      );
    }

    return result as Map<String, dynamic>;
  }

  void _checkProtocolError(MpcRoundResult result) {
    if (result.isError) {
      throw MpcProtocolException(
        result.errorMessage ?? 'Unknown protocol error',
        round: result.round,
      );
    }
  }

  Map<String, dynamic> _snakeToCamelKeys(Map<String, dynamic> map) {
    return map.map((key, value) {
      final camelKey = key.replaceAllMapped(
        RegExp(r'_([a-z])'),
        (m) => m.group(1)!.toUpperCase(),
      );
      return MapEntry(camelKey, value);
    });
  }
}
