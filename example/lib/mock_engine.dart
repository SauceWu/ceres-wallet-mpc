/// Mock MPC engine that simulates Rust protocol rounds without real crypto.
///
/// Use with [MockMpcTransport] to run the example app without a real server
/// or Rust crypto backend. Returns fake round results that drive the
/// MpcClient's start/continue loop to completion.
///
/// For real usage, use [MpcEngine] with [RustLib.instance.api].
library;

import 'dart:convert';
import 'dart:math';
import 'package:ceres_mpc/src/bridge/mpc_engine.dart';
import 'package:ceres_mpc/src/dto/mpc_dtos.dart';

/// Simulates MpcEngine protocol rounds without calling Rust FFI.
///
/// Each protocol (keygen/sign/recover) tracks rounds per session.
/// Returns `status: "continue"` for intermediate rounds and
/// `status: "completed"` on the final round, matching real engine behavior.
class MockMpcEngine implements MpcEngine {
  final _sessionRounds = <String, int>{};
  final _rand = Random(42);

  String _mockHex(int length) {
    const chars = '0123456789abcdef';
    return List.generate(length, (_) => chars[_rand.nextInt(16)]).join();
  }

  /// Build a fake WireEnvelope JSON string (client → server direction).
  String _mockClientPayload(String sessionId, String protocol, int round) {
    final mockBytes = List.generate(64, (_) => _rand.nextInt(256));
    return jsonEncode({
      'session_id': sessionId,
      'protocol': protocol,
      'round': round,
      'from_id': 0,
      'to_id': 1,
      'payload_encoding': 'cbor_base64',
      'payload': base64Encode(mockBytes),
    });
  }

  // ── Keygen ──────────────────────────────────────────────────────

  @override
  Future<MpcRoundResult> keygenStart(
    String sessionId,
    String serverPayload,
  ) async {
    _sessionRounds[sessionId] = 1;
    return MpcRoundResult(
      status: 'continue',
      round: 1,
      clientPayload: _mockClientPayload(sessionId, 'dkg', 1),
    );
  }

  @override
  Future<MpcRoundResult> keygenContinue(
    String sessionId,
    String serverPayload,
  ) async {
    final round = (_sessionRounds[sessionId] ?? 1) + 1;
    _sessionRounds[sessionId] = round;

    if (round < 4) {
      return MpcRoundResult(
        status: 'continue',
        round: round,
        clientPayload: _mockClientPayload(sessionId, 'dkg', round),
      );
    }

    // Final round: completed
    _sessionRounds.remove(sessionId);
    final mockKeyId = 'mpc_${_mockHex(8)}';
    return MpcRoundResult(
      status: 'completed',
      round: round,
      clientPayload: jsonEncode({
        'mpc_key_id': mockKeyId,
        'address': '0x${_mockHex(40)}',
        'public_key': '04${_mockHex(128)}',
        'curve': 'secp256k1',
        'threshold': 2,
        'key_ref': mockKeyId,
        'backup_state': 'pending',
        'rotation_version': 1,
        'local_encrypted_share':
            base64Encode(List.generate(128, (_) => _rand.nextInt(256))),
      }),
    );
  }

  // ── Recovery ────────────────────────────────────────────────────

  @override
  Future<MpcRoundResult> recoverStart(
    String sessionId,
    String backupShare,
    String serverPayload,
    int currentRotationVersion,
  ) async {
    _sessionRounds[sessionId] = 1;
    return MpcRoundResult(
      status: 'continue',
      round: 1,
      clientPayload: _mockClientPayload(sessionId, 'rotation', 1),
    );
  }

  @override
  Future<MpcRoundResult> recoverContinue(
    String sessionId,
    String serverPayload,
  ) async {
    final round = (_sessionRounds[sessionId] ?? 1) + 1;
    _sessionRounds[sessionId] = round;

    if (round < 4) {
      return MpcRoundResult(
        status: 'continue',
        round: round,
        clientPayload: _mockClientPayload(sessionId, 'rotation', round),
      );
    }

    _sessionRounds.remove(sessionId);
    return MpcRoundResult(
      status: 'completed',
      round: round,
      clientPayload: jsonEncode({
        'mpc_key_id': sessionId,
        'address': '0x${_mockHex(40)}',
        'public_key': '04${_mockHex(128)}',
        'curve': 'secp256k1',
        'threshold': 2,
        'key_ref': sessionId,
        'backup_state': 'pending',
        'rotation_version': 2,
        'local_encrypted_share':
            base64Encode(List.generate(128, (_) => _rand.nextInt(256))),
        'encrypted_backup_share': null,
      }),
    );
  }

  // ── Sign ────────────────────────────────────────────────────────

  @override
  Future<MpcRoundResult> signStart(
    String sessionId,
    String share,
    String messageHashHex,
    String serverPayload,
  ) async {
    _sessionRounds[sessionId] = 1;
    return MpcRoundResult(
      status: 'continue',
      round: 1,
      clientPayload: _mockClientPayload(sessionId, 'dsg', 1),
    );
  }

  @override
  Future<MpcRoundResult> signContinue(
    String sessionId,
    String serverPayload,
  ) async {
    final round = (_sessionRounds[sessionId] ?? 1) + 1;
    _sessionRounds[sessionId] = round;

    if (round < 4) {
      return MpcRoundResult(
        status: 'continue',
        round: round,
        clientPayload: _mockClientPayload(sessionId, 'dsg', round),
      );
    }

    _sessionRounds.remove(sessionId);
    return MpcRoundResult(
      status: 'completed',
      round: round,
      clientPayload: jsonEncode({
        'r': _mockHex(64),
        's': _mockHex(64),
        'recid': _rand.nextInt(2),
      }),
    );
  }

  // ── Backup & Export (pass-through, no protocol rounds) ─────────

  @override
  Future<BackupEnvelope> deriveBackupEnvelope(
    String localEncryptedShare,
    String userBackupSecret,
    String createdAt,
  ) async {
    return BackupEnvelope(
      version: '1',
      algorithm: 'AES-256-GCM',
      createdAt: createdAt,
      payload: base64Encode(List.generate(96, (_) => _rand.nextInt(256))),
    );
  }

  @override
  Future<String> decryptBackupShare(
    String encryptedEnvelope,
    String userBackupSecret,
  ) async {
    return base64Encode(List.generate(128, (_) => _rand.nextInt(256)));
  }

  @override
  Future<String> exportPrivateKey(
    String localShare,
    String serverSharePrivate,
  ) async {
    return _mockHex(64); // 32-byte private key hex
  }
}
