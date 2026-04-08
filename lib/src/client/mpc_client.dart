import 'dart:convert';

import '../bridge/mpc_engine.dart';
import '../dto/mpc_dtos.dart';
import '../transport/mpc_transport.dart';
import 'mpc_exceptions.dart';

/// High-level MPC orchestration API for host applications.
///
/// Drives keygen/recovery round-trips by coordinating between
/// [MpcEngine] (Rust FFI) and [MpcTransport] (host-injected network).
class MpcClient {
  final MpcEngine _engine;
  final MpcTransport _transport;

  MpcClient({required MpcEngine engine, required MpcTransport transport})
      : _engine = engine,
        _transport = transport;

  /// Execute full keygen protocol.
  ///
  /// 1. Calls transport to initiate keygen on server
  /// 2. Calls Rust keygen_start with server's first message
  /// 3. Sends client payload to server, receives response,
  ///    calls Rust keygen_continue until completed
  /// 4. Returns [KeygenResult] with address, publicKey, localEncryptedShare
  Future<KeygenResult> keygen() async {
    final initResponse = await _sendToServer('/keygen/start', '{}');
    final initData = _parseServerResponse(initResponse);
    final sessionId = initData['sessionId'] as String;

    final round1 = await _engine.keygenStart(
      sessionId,
      jsonEncode(initData['serverPayload']),
    );
    _checkProtocolError(round1);

    var currentResult = round1;
    while (currentResult.isContinue) {
      final serverResponse = await _sendToServer(
        '/keygen/continue',
        jsonEncode({
          'sessionId': sessionId,
          'round': currentResult.round,
          'clientPayload': currentResult.clientPayload,
        }),
      );
      final serverData = _parseServerResponse(serverResponse);

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
  ///
  /// Decrypts the backup share, then drives recovery/rotation rounds
  /// with the server. Returns [RecoveryResult] with new localEncryptedShare
  /// and incremented rotationVersion.
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

    final initResponse = await _sendToServer(
      '/recovery/start',
      jsonEncode({'mpcKeyId': mpcKeyId}),
    );
    final initData = _parseServerResponse(initResponse);
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
      final serverResponse = await _sendToServer(
        '/recovery/continue',
        jsonEncode({
          'sessionId': sessionId,
          'round': currentResult.round,
          'clientPayload': currentResult.clientPayload,
        }),
      );
      final serverData = _parseServerResponse(serverResponse);

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

      // Generate new backup after rotation if secret provided
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
  ///
  /// Per D-01: [messageHash] is raw 32-byte hash hex (no 0x prefix).
  /// Per D-03: [localEncryptedShare] is passed by caller — SDK is stateless.
  /// Per D-02: Returns [SignResult] with r, s, recid.
  Future<SignResult> sign({
    required String mpcKeyId,
    required String messageHash,
    required String localEncryptedShare,
  }) async {
    final initResponse = await _sendToServer(
      '/sign/start',
      jsonEncode({'mpcKeyId': mpcKeyId, 'messageHash': messageHash}),
    );
    final initData = _parseServerResponse(initResponse);
    final sessionId = initData['sessionId'] as String;

    final round1 = await _engine.signStart(
      sessionId,
      localEncryptedShare,
      jsonEncode(initData['serverPayload']),
    );
    _checkProtocolError(round1);

    var currentResult = round1;
    while (currentResult.isContinue) {
      final serverResponse = await _sendToServer(
        '/sign/continue',
        jsonEncode({
          'sessionId': sessionId,
          'round': currentResult.round,
          'clientPayload': currentResult.clientPayload,
        }),
      );
      final serverData = _parseServerResponse(serverResponse);

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

  Future<String> _sendToServer(String endpoint, String payload) async {
    try {
      return await _transport.send(endpoint, payload);
    } catch (e) {
      throw MpcTransportException(
        'Transport failed: $e',
        endpoint: endpoint,
        cause: e,
      );
    }
  }

  Map<String, dynamic> _parseServerResponse(String response) {
    try {
      return jsonDecode(response) as Map<String, dynamic>;
    } catch (e) {
      throw MpcProtocolException('Invalid server response JSON: $e');
    }
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
