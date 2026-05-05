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
  final _sessionCurves = <String, String>{};
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

  /// Returns a SharedEnvelope v2 JSON string for ed25519 key shares.
  String _mockShareEnvelope() {
    return jsonEncode({
      'v': 2,
      'curve': 'ed25519',
      'share': base64Encode(List.generate(32, (_) => _rand.nextInt(256))),
    });
  }

  /// Parses the curve from a serverPayload JSON string.
  /// Returns the 'curve' field if present, otherwise 'secp256k1'.
  String _parseCurve(String serverPayload) {
    try {
      final Map<String, dynamic> parsed;
      final decoded = jsonDecode(serverPayload);
      if (decoded is Map<String, dynamic>) {
        parsed = decoded;
      } else {
        return 'secp256k1';
      }
      return parsed['curve'] as String? ?? 'secp256k1';
    } catch (_) {
      return 'secp256k1';
    }
  }

  MpcRoundResult _roundOrComplete(
    String sessionId,
    String protocol,
    int round,
    int maxRounds,
    Map<String, dynamic> Function() completedPayload,
  ) {
    _sessionRounds[sessionId] = round;
    if (round < maxRounds) {
      return MpcRoundResult(
        status: 'continue',
        round: round,
        clientPayload: _mockClientPayload(sessionId, protocol, round),
      );
    }
    _sessionRounds.remove(sessionId);
    _sessionCurves.remove(sessionId);
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
    // At round 1, detect and store the curve for this session
    if (round == 1) {
      final curve = _parseCurve(serverPayload);
      _sessionCurves[sessionId] = curve;
    }

    final curve = _sessionCurves[sessionId] ?? 'secp256k1';
    final isSol = curve == 'ed25519';
    final maxRounds = isSol ? 3 : 4;
    final mockKeyId = 'mpc_${_mockHex(8)}';

    return _roundOrComplete(sessionId, 'dkg', round, maxRounds, () {
      if (isSol) {
        const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
        final solAddr = List.generate(44, (_) => alphabet[_rand.nextInt(alphabet.length)]).join();
        return {
          'mpc_key_id': mockKeyId,
          'address': solAddr,
          'public_key': _mockHex(64),
          'curve': 'ed25519',
          'threshold': 2,
          'key_ref': mockKeyId,
          'backup_state': 'pending',
          'rotation_version': 1,
          'local_encrypted_share': _mockShareEnvelope(),
        };
      } else {
        return {
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
        };
      }
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
    // At round 1, detect and store the curve for this session
    if (round == 1) {
      final curve = _parseCurve(serverPayload);
      _sessionCurves[sessionId] = curve;
    }

    final curve = _sessionCurves[sessionId] ?? 'secp256k1';
    final isSol = curve == 'ed25519';
    final maxRounds = isSol ? 3 : 4;

    return _roundOrComplete(sessionId, 'rotation', round, maxRounds, () {
      if (isSol) {
        const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
        final solAddr = List.generate(44, (_) => alphabet[_rand.nextInt(alphabet.length)]).join();
        return {
          'mpc_key_id': sessionId,
          'address': solAddr,
          'public_key': _mockHex(64),
          'curve': 'ed25519',
          'threshold': 2,
          'key_ref': sessionId,
          'backup_state': 'pending',
          'rotation_version': (currentRotationVersion ?? 1) + 1,
          'local_encrypted_share': _mockShareEnvelope(),
          'encrypted_backup_share': null,
        };
      } else {
        return {
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
        };
      }
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
    // At round 1, detect and store the curve for this session
    if (round == 1) {
      final curve = _parseCurve(serverPayload);
      _sessionCurves[sessionId] = curve;
    }

    final curve = _sessionCurves[sessionId] ?? 'secp256k1';
    final isSol = curve == 'ed25519';
    final maxRounds = isSol ? 2 : 4;

    return _roundOrComplete(sessionId, 'dsg', round, maxRounds, () {
      if (isSol) {
        // FROST sign completion: no recid, curve tag required.
        // NOTE: SignResult.fromJson expects no snake_case conversion for sign.
        return {
          'r': _mockHex(64),
          's': _mockHex(64),
          'recid': null,
          'curve': 'ed25519',
        };
      } else {
        return {
          'r': _mockHex(64),
          's': _mockHex(64),
          'recid': _rand.nextInt(2),
        };
      }
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
