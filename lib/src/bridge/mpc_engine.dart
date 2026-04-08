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
  ) async {
    final result = await _api.crateApiMpcEngineRecoverStart(
      sessionId: sessionId,
      backupShare: backupShare,
      serverPayload: serverPayload,
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
}
