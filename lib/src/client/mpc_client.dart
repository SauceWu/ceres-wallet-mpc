import 'dart:convert';

import '../bridge/mpc_engine.dart';
import '../dto/mpc_dtos.dart';
import '../transport/mpc_transport.dart';
import 'mpc_exceptions.dart';

/// JSON-RPC 2.0 method names for MPC protocol operations.
/// Merged: keygen_start/continue → keygen, sign_start/continue → sign, etc.
enum MpcMethod {
  keygen('keygen'),
  recovery('recovery'),
  sign('sign'),
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

  /// Execute full keygen protocol (4-round DKLs23 DKG).
  Future<KeygenResult> keygen() async {
    // Round 1: initiate
    final initData = await _rpcCall(MpcMethod.keygen, {'round': 1});
    final sessionId = initData['sessionId'] as String;

    var currentResult = await _engine.keygen(
      sessionId, 1, jsonEncode(initData['serverPayload']),
    );
    _checkProtocolError(currentResult);

    // Rounds 2+: loop until engine or server signals completion
    int round = 2;
    while (currentResult.isContinue) {
      final serverData = await _rpcCall(MpcMethod.keygen, {
        'sessionId': sessionId,
        'round': round,
        'clientPayload': currentResult.clientPayload,
      });

      final serverPayload = serverData['serverPayload'];
      if (serverPayload != null) {
        currentResult = await _engine.keygen(
          sessionId, round, jsonEncode(serverPayload),
        );
        _checkProtocolError(currentResult);
      } else {
        // Server completed (no more serverPayload) — collect Keyshare from engine
        // round=0 tells Rust to join the protocol task and return the Keyshare
        currentResult = await _engine.keygen(sessionId, 0, '');
        _checkProtocolError(currentResult);
        break;
      }
      round++;
    }

    if (currentResult.isCompleted && currentResult.clientPayload != null) {
      final payload =
          jsonDecode(currentResult.clientPayload!) as Map<String, dynamic>;
      return KeygenResult.fromJson(_snakeToCamelKeys(payload));
    }

    throw MpcProtocolException(
        'Unexpected keygen state: ${currentResult.status}');
  }

  /// Execute full recovery protocol (4-round DKLs23 key_refresh).
  Future<RecoveryResult> recover({
    required String mpcKeyId,
    required String encryptedBackupShare,
    required String userBackupSecret,
    required int currentRotationVersion,
    String? newUserBackupSecret,
  }) async {
    final backupShare = await _engine.decryptBackupShare(
      encryptedBackupShare, userBackupSecret,
    );

    // Round 1: initiate with backup_share
    final initData = await _rpcCall(MpcMethod.recovery, {
      'mpcKeyId': mpcKeyId,
      'round': 1,
    });
    final sessionId = initData['sessionId'] as String;

    var currentResult = await _engine.recover(
      sessionId, 1, jsonEncode(initData['serverPayload']),
      backupShare: backupShare,
      currentRotationVersion: currentRotationVersion,
    );
    _checkProtocolError(currentResult);

    // Rounds 2+
    int round = 2;
    while (currentResult.isContinue) {
      final serverData = await _rpcCall(MpcMethod.recovery, {
        'sessionId': sessionId,
        'round': round,
        'clientPayload': currentResult.clientPayload,
      });

      final serverPayload = serverData['serverPayload'];
      if (serverPayload != null) {
        currentResult = await _engine.recover(
          sessionId, round, jsonEncode(serverPayload),
        );
        _checkProtocolError(currentResult);
      } else {
        break;
      }
      round++;
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

  /// Execute full sign protocol (4-round DKLs23 DSG).
  Future<SignResult> sign({
    required String mpcKeyId,
    required String messageHash,
    required String localEncryptedShare,
  }) async {
    // Round 1: initiate with share + hash
    final initData = await _rpcCall(MpcMethod.sign, {
      'mpcKeyId': mpcKeyId,
      'messageHash': messageHash,
      'round': 1,
    });
    final sessionId = initData['sessionId'] as String;

    var currentResult = await _engine.sign(
      sessionId, 1, jsonEncode(initData['serverPayload']),
      share: localEncryptedShare,
      messageHashHex: messageHash,
    );
    _checkProtocolError(currentResult);

    // Rounds 2+
    int round = 2;
    while (currentResult.isContinue) {
      final serverData = await _rpcCall(MpcMethod.sign, {
        'sessionId': sessionId,
        'round': round,
        'clientPayload': currentResult.clientPayload,
      });

      final serverPayload = serverData['serverPayload'];
      if (serverPayload != null) {
        currentResult = await _engine.sign(
          sessionId, round, jsonEncode(serverPayload),
        );
        _checkProtocolError(currentResult);
      } else {
        // Server completed — check if it returned sign result directly
        if (serverData['status'] == 'completed' && serverData['r'] != null) {
          return SignResult.fromJson(serverData);
        }
        break;
      }
      round++;
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
      localEncryptedShare, serverSharePrivate,
    );
    final payload = jsonDecode(resultJson) as Map<String, dynamic>;
    return ExportResult.fromJson(_snakeToCamelKeys(payload));
  }

  // ── JSON-RPC 2.0 helpers ──────────────────────────────────────

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

    if (response.containsKey('error') && response['error'] != null) {
      final error = response['error'] as Map<String, dynamic>;
      throw MpcRpcException(
        code: error['code'] as int,
        message: error['message'] as String,
        data: error['data'],
      );
    }

    final result = response['result'];
    if (result == null) {
      throw MpcProtocolException('JSON-RPC response missing "result" field');
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
