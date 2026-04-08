import 'dart:convert';

import '../dto/mpc_dtos.dart';
import '../rust/frb_generated.dart';

/// Internal Rust FFI wrapper — NOT exposed to host apps (per D-06).
///
/// Wraps the FRB-generated [RustLibApi] with typed Dart methods that
/// deserialize JSON responses into [MpcRoundResult]. Errors from the
/// Rust side are rethrown for the upstream MpcClient layer to handle.
class MpcEngine {
  final RustLibApi _api;

  MpcEngine(this._api);

  Future<MpcRoundResult> keygenStart(
    String sessionId,
    String serverPayload,
  ) async {
    final result = await _api.crateApiMpcEngineKeygenStart(
      sessionId: sessionId,
      serverPayload: serverPayload,
    );
    return MpcRoundResult.fromJson(
      jsonDecode(result) as Map<String, dynamic>,
    );
  }

  Future<MpcRoundResult> keygenContinue(
    String sessionId,
    String serverPayload,
  ) async {
    final result = await _api.crateApiMpcEngineKeygenContinue(
      sessionId: sessionId,
      serverPayload: serverPayload,
    );
    return MpcRoundResult.fromJson(
      jsonDecode(result) as Map<String, dynamic>,
    );
  }

  Future<MpcRoundResult> recoverStart(
    String sessionId,
    String backupShare,
    String serverPayload,
    int currentRotationVersion,
  ) async {
    final result = await _api.crateApiMpcEngineRecoverStart(
      sessionId: sessionId,
      backupShare: backupShare,
      serverPayload: serverPayload,
      currentRotationVersion: currentRotationVersion,
    );
    return MpcRoundResult.fromJson(
      jsonDecode(result) as Map<String, dynamic>,
    );
  }

  Future<MpcRoundResult> recoverContinue(
    String sessionId,
    String serverPayload,
  ) async {
    final result = await _api.crateApiMpcEngineRecoverContinue(
      sessionId: sessionId,
      serverPayload: serverPayload,
    );
    return MpcRoundResult.fromJson(
      jsonDecode(result) as Map<String, dynamic>,
    );
  }

  Future<MpcRoundResult> signStart(
    String sessionId,
    String share,
    String serverPayload,
  ) async {
    final result = await _api.crateApiMpcEngineSignStart(
      sessionId: sessionId,
      share: share,
      serverPayload: serverPayload,
    );
    return MpcRoundResult.fromJson(
      jsonDecode(result) as Map<String, dynamic>,
    );
  }

  Future<MpcRoundResult> signContinue(
    String sessionId,
    String serverPayload,
  ) async {
    final result = await _api.crateApiMpcEngineSignContinue(
      sessionId: sessionId,
      serverPayload: serverPayload,
    );
    return MpcRoundResult.fromJson(
      jsonDecode(result) as Map<String, dynamic>,
    );
  }

  /// Derive a backup envelope from a live share and user secret.
  /// Uses AES-256-GCM with HKDF-SHA256 key derivation.
  Future<BackupEnvelope> deriveBackupEnvelope(
    String localEncryptedShare,
    String userBackupSecret,
    String createdAt,
  ) async {
    final result = await _api.crateApiMpcEngineDeriveBackupEnvelope(
      localEncryptedShare: localEncryptedShare,
      userBackupSecret: userBackupSecret,
      createdAt: createdAt,
    );
    return BackupEnvelope.fromJson(
      jsonDecode(result) as Map<String, dynamic>,
    );
  }

  /// Decrypt a backup envelope to recover the device backup share.
  /// Phase 2 stub — real decryption in Phase 5.
  /// Per D-08: returns opaque deviceBackupShare string for recovery flow.
  Future<String> decryptBackupShare(
    String encryptedEnvelope,
    String userBackupSecret,
  ) async {
    final result = await _api.crateApiMpcEngineDecryptBackupShare(
      encryptedEnvelope: encryptedEnvelope,
      userBackupSecret: userBackupSecret,
    );
    return (jsonDecode(result) as Map<String, dynamic>)['device_backup_share']
        as String;
  }

  /// Export full private key by combining Party1 and Party2 shares.
  Future<String> exportPrivateKey(
    String localShare,
    String serverSharePrivate,
  ) async {
    final result = await _api.crateApiMpcEngineExportPrivateKey(
      localShare: localShare,
      serverSharePrivate: serverSharePrivate,
    );
    return result;
  }
}
