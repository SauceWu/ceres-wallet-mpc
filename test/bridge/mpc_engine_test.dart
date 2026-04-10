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

  test('keygen round 1 returns valid MpcRoundResult', () async {
    when(
      () => mockApi.crateApiMpcEngineKeygen(
        sessionId: any(named: 'sessionId'),
        round: any(named: 'round'),
        serverPayload: any(named: 'serverPayload'),
      ),
    ).thenAnswer(
      (_) async => _roundJson(clientPayload: 'stub_keygen_round1'),
    );

    final result = await engine.keygen('sess1', 1, '{}');

    expect(result.status, 'continue');
    expect(result.round, 1);
    expect(result.clientPayload, contains('stub_keygen'));
    expect(result.isContinue, isTrue);
  });

  test('keygen returns completed status on final round', () async {
    when(
      () => mockApi.crateApiMpcEngineKeygen(
        sessionId: any(named: 'sessionId'),
        round: any(named: 'round'),
        serverPayload: any(named: 'serverPayload'),
      ),
    ).thenAnswer(
      (_) async => _roundJson(
        status: 'completed',
        round: 4,
        clientPayload: 'stub_keygen_completed',
      ),
    );

    final result = await engine.keygen('sess1', 4, '{}');

    expect(result.isCompleted, isTrue);
    expect(result.round, 4);
  });

  test('recover passes backupShare and rotationVersion on round 1', () async {
    when(
      () => mockApi.crateApiMpcEngineRecover(
        sessionId: any(named: 'sessionId'),
        round: any(named: 'round'),
        serverPayload: any(named: 'serverPayload'),
        backupShare: any(named: 'backupShare'),
        currentRotationVersion: any(named: 'currentRotationVersion'),
      ),
    ).thenAnswer(
      (_) async => _roundJson(clientPayload: 'stub_recover_round1'),
    );

    final result = await engine.recover(
      'sess1', 1, '{}',
      backupShare: 'backup_data',
      currentRotationVersion: 1,
    );

    expect(result.isContinue, isTrue);
    verify(
      () => mockApi.crateApiMpcEngineRecover(
        sessionId: 'sess1',
        round: 1,
        serverPayload: '{}',
        backupShare: 'backup_data',
        currentRotationVersion: 1,
      ),
    ).called(1);
  });

  test('sign passes share and messageHashHex on round 1', () async {
    when(
      () => mockApi.crateApiMpcEngineSign(
        sessionId: any(named: 'sessionId'),
        round: any(named: 'round'),
        serverPayload: any(named: 'serverPayload'),
        share: any(named: 'share'),
        messageHashHex: any(named: 'messageHashHex'),
      ),
    ).thenAnswer(
      (_) async => _roundJson(clientPayload: 'stub_sign_round1'),
    );

    const dummyHash = 'aabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccdd';
    final result = await engine.sign(
      'sess1', 1, '{}',
      share: 'share_data',
      messageHashHex: dummyHash,
    );

    expect(result.isContinue, isTrue);
    verify(
      () => mockApi.crateApiMpcEngineSign(
        sessionId: 'sess1',
        round: 1,
        serverPayload: '{}',
        share: 'share_data',
        messageHashHex: dummyHash,
      ),
    ).called(1);
  });

  test('invalid JSON from FRB throws FormatException', () async {
    when(
      () => mockApi.crateApiMpcEngineKeygen(
        sessionId: any(named: 'sessionId'),
        round: any(named: 'round'),
        serverPayload: any(named: 'serverPayload'),
      ),
    ).thenAnswer((_) async => 'not-json');

    expect(
      () => engine.keygen('sess1', 1, '{}'),
      throwsA(isA<FormatException>()),
    );
  });

  test('all 3 protocol methods callable through MpcEngine', () async {
    when(
      () => mockApi.crateApiMpcEngineKeygen(
        sessionId: any(named: 'sessionId'),
        round: any(named: 'round'),
        serverPayload: any(named: 'serverPayload'),
      ),
    ).thenAnswer((_) async => _roundJson());

    when(
      () => mockApi.crateApiMpcEngineRecover(
        sessionId: any(named: 'sessionId'),
        round: any(named: 'round'),
        serverPayload: any(named: 'serverPayload'),
        backupShare: any(named: 'backupShare'),
        currentRotationVersion: any(named: 'currentRotationVersion'),
      ),
    ).thenAnswer((_) async => _roundJson());

    when(
      () => mockApi.crateApiMpcEngineSign(
        sessionId: any(named: 'sessionId'),
        round: any(named: 'round'),
        serverPayload: any(named: 'serverPayload'),
        share: any(named: 'share'),
        messageHashHex: any(named: 'messageHashHex'),
      ),
    ).thenAnswer((_) async => _roundJson());

    await engine.keygen('s', 1, '{}');
    await engine.recover('s', 1, '{}', backupShare: 'b', currentRotationVersion: 1);
    await engine.sign('s', 1, '{}', share: 'sh', messageHashHex: '0' * 64);

    verify(() => mockApi.crateApiMpcEngineKeygen(
      sessionId: any(named: 'sessionId'),
      round: any(named: 'round'),
      serverPayload: any(named: 'serverPayload'),
    )).called(1);
    verify(() => mockApi.crateApiMpcEngineRecover(
      sessionId: any(named: 'sessionId'),
      round: any(named: 'round'),
      serverPayload: any(named: 'serverPayload'),
      backupShare: any(named: 'backupShare'),
      currentRotationVersion: any(named: 'currentRotationVersion'),
    )).called(1);
    verify(() => mockApi.crateApiMpcEngineSign(
      sessionId: any(named: 'sessionId'),
      round: any(named: 'round'),
      serverPayload: any(named: 'serverPayload'),
      share: any(named: 'share'),
      messageHashHex: any(named: 'messageHashHex'),
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
    expect(result.payload, 'stub_envelope_share_abc');
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
  });
}
