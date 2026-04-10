import 'dart:convert';

import '../dto/mpc_dtos.dart';
import '../rust/frb_generated.dart';

/// Internal Rust FFI wrapper — NOT exposed to host apps.
///
/// Wraps the FRB-generated [RustLibApi] with typed Dart methods that
/// deserialize JSON responses into [MpcRoundResult].
class MpcEngine {
  final RustLibApi _api;

  MpcEngine(this._api);

  /// DKG 协议统一入口。round==1 创建 session，round>1 推进。
  Future<MpcRoundResult> keygen(
    String sessionId,
    int round,
    String serverPayload,
  ) async {
    final result = await _api.crateApiMpcEngineKeygen(
      sessionId: sessionId,
      round: round,
      serverPayload: serverPayload,
    );
    return MpcRoundResult.fromJson(jsonDecode(result) as Map<String, dynamic>);
  }

  /// key_refresh 协议统一入口。round==1 需要额外参数。
  Future<MpcRoundResult> recover(
    String sessionId,
    int round,
    String serverPayload, {
    String? backupShare,
    int? currentRotationVersion,
  }) async {
    final result = await _api.crateApiMpcEngineRecover(
      sessionId: sessionId,
      round: round,
      serverPayload: serverPayload,
      backupShare: backupShare,
      currentRotationVersion: currentRotationVersion,
    );
    return MpcRoundResult.fromJson(jsonDecode(result) as Map<String, dynamic>);
  }

  /// DSG 协议统一入口。round==1 需要额外参数。
  Future<MpcRoundResult> sign(
    String sessionId,
    int round,
    String serverPayload, {
    String? share,
    String? messageHashHex,
  }) async {
    final result = await _api.crateApiMpcEngineSign(
      sessionId: sessionId,
      round: round,
      serverPayload: serverPayload,
      share: share,
      messageHashHex: messageHashHex,
    );
    return MpcRoundResult.fromJson(jsonDecode(result) as Map<String, dynamic>);
  }

  /// Derive a backup envelope from a live share and user secret.
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
    return BackupEnvelope.fromJson(jsonDecode(result) as Map<String, dynamic>);
  }

  /// Decrypt a backup envelope to recover the device backup share.
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
