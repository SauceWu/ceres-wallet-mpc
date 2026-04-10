/// Mock MPC engine that simulates Rust protocol rounds without real crypto.
///
/// Use with [MockMpcTransport] to run the example app without a real server
/// or Rust crypto backend. Returns fake round results that drive the
/// MpcClient's round loop to completion.
///
/// For real usage, use [MpcEngine] with [RustLib.instance.api].
library;

// ignore_for_file: implementation_imports

import 'dart:convert';
import 'dart:math';
import 'package:ceres_mpc/src/bridge/mpc_engine.dart';
import 'package:ceres_mpc/src/dto/mpc_dtos.dart';

/// Simulates MpcEngine protocol rounds without calling Rust FFI.
///
/// Uses the merged 3-function API: keygen(round), sign(round), recover(round).
/// Returns `status: "continue"` for intermediate rounds and
/// `status: "completed"` on the final round.
class MockMpcEngine implements MpcEngine {
  final _sessionRounds = <String, int>{};
  final _rand = Random(42);

  String _mockHex(int length) {
    const chars = '0123456789abcdef';
    return List.generate(length, (_) => chars[_rand.nextInt(16)]).join();
  }

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

  MpcRoundResult _roundOrComplete(
    String sessionId,
    String protocol,
    int round,
    Map<String, dynamic> Function() completedPayload,
  ) {
    _sessionRounds[sessionId] = round;
    if (round < 4) {
      return MpcRoundResult(
        status: 'continue',
        round: round,
        clientPayload: _mockClientPayload(sessionId, protocol, round),
      );
    }
    _sessionRounds.remove(sessionId);
    return MpcRoundResult(
      status: 'completed',
      round: round,
      clientPayload: jsonEncode(completedPayload()),
    );
  }

  // ── Keygen ──────────────────────────────────────────────────────

  @override
  Future<MpcRoundResult> keygen(
    String sessionId,
    int round,
    String serverPayload,
  ) async {
    final mockKeyId = 'mpc_${_mockHex(8)}';
    return _roundOrComplete(sessionId, 'dkg', round, () => {
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
    });
  }

  // ── Recovery ────────────────────────────────────────────────────

  @override
  Future<MpcRoundResult> recover(
    String sessionId,
    int round,
    String serverPayload, {
    String? backupShare,
    int? currentRotationVersion,
  }) async {
    return _roundOrComplete(sessionId, 'rotation', round, () => {
      'mpc_key_id': sessionId,
      'address': '0x${_mockHex(40)}',
      'public_key': '04${_mockHex(128)}',
      'curve': 'secp256k1',
      'threshold': 2,
      'key_ref': sessionId,
      'backup_state': 'pending',
      'rotation_version': (currentRotationVersion ?? 1) + 1,
      'local_encrypted_share':
          base64Encode(List.generate(128, (_) => _rand.nextInt(256))),
      'encrypted_backup_share': null,
    });
  }

  // ── Sign ────────────────────────────────────────────────────────

  @override
  Future<MpcRoundResult> sign(
    String sessionId,
    int round,
    String serverPayload, {
    String? share,
    String? messageHashHex,
  }) async {
    return _roundOrComplete(sessionId, 'dsg', round, () => {
      'r': _mockHex(64),
      's': _mockHex(64),
      'recid': _rand.nextInt(2),
    });
  }

  // ── Backup & Export ─────────────────────────────────────────────

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
    return _mockHex(64);
  }
}
