import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'package:ceres_mpc/src/bridge/mpc_engine.dart';
import 'package:ceres_mpc/src/dto/mpc_dtos.dart';
import 'package:ceres_mpc/src/rust/frb_generated.dart';

class MockRustLibApi extends Mock implements RustLibApi {}

String _roundJson({
  String status = 'continue',
  int round = 1,
  String? clientPayload,
  String? errorMessage,
}) {
  return jsonEncode({
    'status': status,
    'round': round,
    'client_payload': clientPayload,
    'error_message': errorMessage,
  });
}

void main() {
  late MockRustLibApi mockApi;
  late MpcEngine engine;

  setUp(() {
    mockApi = MockRustLibApi();
    engine = MpcEngine(mockApi);
  });

  test('keygenStart returns valid MpcRoundResult', () async {
    when(
      () => mockApi.crateApiMpcEngineKeygenStart(
        sessionId: any(named: 'sessionId'),
        serverPayload: any(named: 'serverPayload'),
      ),
    ).thenAnswer(
      (_) async => _roundJson(
        clientPayload: 'stub_keygen_round1_sess1',
      ),
    );

    final result = await engine.keygenStart('sess1', '{}');

    expect(result.status, 'continue');
    expect(result.round, 1);
    expect(result.clientPayload, contains('stub_keygen'));
    expect(result.isContinue, isTrue);
  });

  test('keygenContinue returns completed status', () async {
    when(
      () => mockApi.crateApiMpcEngineKeygenContinue(
        sessionId: any(named: 'sessionId'),
        serverPayload: any(named: 'serverPayload'),
      ),
    ).thenAnswer(
      (_) async => _roundJson(
        status: 'completed',
        round: 2,
        clientPayload: 'stub_keygen_completed_sess1',
      ),
    );

    final result = await engine.keygenContinue('sess1', '{}');

    expect(result.isCompleted, isTrue);
    expect(result.round, 2);
  });

  test('recoverStart passes backupShare parameter correctly', () async {
    when(
      () => mockApi.crateApiMpcEngineRecoverStart(
        sessionId: any(named: 'sessionId'),
        backupShare: any(named: 'backupShare'),
        serverPayload: any(named: 'serverPayload'),
        currentRotationVersion: any(named: 'currentRotationVersion'),
      ),
    ).thenAnswer(
      (_) async => _roundJson(clientPayload: 'stub_recover_round1'),
    );

    final result = await engine.recoverStart('sess1', 'backup_data', '{}', 1);

    expect(result.isContinue, isTrue);
    verify(
      () => mockApi.crateApiMpcEngineRecoverStart(
        sessionId: 'sess1',
        backupShare: 'backup_data',
        serverPayload: '{}',
        currentRotationVersion: 1,
      ),
    ).called(1);
  });

  test('signStart passes share parameter correctly', () async {
    when(
      () => mockApi.crateApiMpcEngineSignStart(
        sessionId: any(named: 'sessionId'),
        share: any(named: 'share'),
        serverPayload: any(named: 'serverPayload'),
      ),
    ).thenAnswer(
      (_) async => _roundJson(clientPayload: 'stub_sign_round1'),
    );

    final result = await engine.signStart('sess1', 'share_data', '{}');

    expect(result.isContinue, isTrue);
    verify(
      () => mockApi.crateApiMpcEngineSignStart(
        sessionId: 'sess1',
        share: 'share_data',
        serverPayload: '{}',
      ),
    ).called(1);
  });

  test('invalid JSON from FRB throws FormatException', () async {
    when(
      () => mockApi.crateApiMpcEngineKeygenStart(
        sessionId: any(named: 'sessionId'),
        serverPayload: any(named: 'serverPayload'),
      ),
    ).thenAnswer((_) async => 'not-json');

    expect(
      () => engine.keygenStart('sess1', '{}'),
      throwsA(isA<FormatException>()),
    );
  });

  test('all 6 methods callable through MpcEngine', () async {
    when(
      () => mockApi.crateApiMpcEngineKeygenStart(
        sessionId: any(named: 'sessionId'),
        serverPayload: any(named: 'serverPayload'),
      ),
    ).thenAnswer((_) async => _roundJson());

    when(
      () => mockApi.crateApiMpcEngineKeygenContinue(
        sessionId: any(named: 'sessionId'),
        serverPayload: any(named: 'serverPayload'),
      ),
    ).thenAnswer((_) async => _roundJson());

    when(
      () => mockApi.crateApiMpcEngineRecoverStart(
        sessionId: any(named: 'sessionId'),
        backupShare: any(named: 'backupShare'),
        serverPayload: any(named: 'serverPayload'),
        currentRotationVersion: any(named: 'currentRotationVersion'),
      ),
    ).thenAnswer((_) async => _roundJson());

    when(
      () => mockApi.crateApiMpcEngineRecoverContinue(
        sessionId: any(named: 'sessionId'),
        serverPayload: any(named: 'serverPayload'),
      ),
    ).thenAnswer((_) async => _roundJson());

    when(
      () => mockApi.crateApiMpcEngineSignStart(
        sessionId: any(named: 'sessionId'),
        share: any(named: 'share'),
        serverPayload: any(named: 'serverPayload'),
      ),
    ).thenAnswer((_) async => _roundJson());

    when(
      () => mockApi.crateApiMpcEngineSignContinue(
        sessionId: any(named: 'sessionId'),
        serverPayload: any(named: 'serverPayload'),
      ),
    ).thenAnswer((_) async => _roundJson());

    await engine.keygenStart('s', '{}');
    await engine.keygenContinue('s', '{}');
    await engine.recoverStart('s', 'b', '{}', 1);
    await engine.recoverContinue('s', '{}');
    await engine.signStart('s', 'sh', '{}');
    await engine.signContinue('s', '{}');

    verify(() => mockApi.crateApiMpcEngineKeygenStart(
          sessionId: any(named: 'sessionId'),
          serverPayload: any(named: 'serverPayload'),
        )).called(1);
    verify(() => mockApi.crateApiMpcEngineKeygenContinue(
          sessionId: any(named: 'sessionId'),
          serverPayload: any(named: 'serverPayload'),
        )).called(1);
    verify(() => mockApi.crateApiMpcEngineRecoverStart(
          sessionId: any(named: 'sessionId'),
          backupShare: any(named: 'backupShare'),
          serverPayload: any(named: 'serverPayload'),
          currentRotationVersion: any(named: 'currentRotationVersion'),
        )).called(1);
    verify(() => mockApi.crateApiMpcEngineRecoverContinue(
          sessionId: any(named: 'sessionId'),
          serverPayload: any(named: 'serverPayload'),
        )).called(1);
    verify(() => mockApi.crateApiMpcEngineSignStart(
          sessionId: any(named: 'sessionId'),
          share: any(named: 'share'),
          serverPayload: any(named: 'serverPayload'),
        )).called(1);
    verify(() => mockApi.crateApiMpcEngineSignContinue(
          sessionId: any(named: 'sessionId'),
          serverPayload: any(named: 'serverPayload'),
        )).called(1);
  });

  test('deriveBackupEnvelope returns BackupEnvelope', () async {
    final envelopeJson = jsonEncode({
      'version': '1',
      'algorithm': 'stub',
      'created_at': '1970-01-01T00:00:00Z',
      'payload': 'stub_envelope_share_abc',
    });

    when(
      () => mockApi.crateApiMpcEngineDeriveBackupEnvelope(
        localEncryptedShare: any(named: 'localEncryptedShare'),
        userBackupSecret: any(named: 'userBackupSecret'),
        createdAt: any(named: 'createdAt'),
      ),
    ).thenAnswer((_) async => envelopeJson);

    final result = await engine.deriveBackupEnvelope(
        'share_abc', 'secret_xyz', '1970-01-01T00:00:00Z');

    expect(result, isA<BackupEnvelope>());
    expect(result.version, '1');
    expect(result.algorithm, 'stub');
    expect(result.createdAt, '1970-01-01T00:00:00Z');
    expect(result.payload, 'stub_envelope_share_abc');

    verify(
      () => mockApi.crateApiMpcEngineDeriveBackupEnvelope(
        localEncryptedShare: 'share_abc',
        userBackupSecret: 'secret_xyz',
        createdAt: '1970-01-01T00:00:00Z',
      ),
    ).called(1);
  });

  test('decryptBackupShare returns opaque device backup share string', () async {
    final decryptJson = jsonEncode({
      'device_backup_share': 'stub_decrypted_envelope_data',
    });

    when(
      () => mockApi.crateApiMpcEngineDecryptBackupShare(
        encryptedEnvelope: any(named: 'encryptedEnvelope'),
        userBackupSecret: any(named: 'userBackupSecret'),
      ),
    ).thenAnswer((_) async => decryptJson);

    final result = await engine.decryptBackupShare('envelope_data', 'secret_xyz');

    expect(result, 'stub_decrypted_envelope_data');

    verify(
      () => mockApi.crateApiMpcEngineDecryptBackupShare(
        encryptedEnvelope: 'envelope_data',
        userBackupSecret: 'secret_xyz',
      ),
    ).called(1);
  });
}
