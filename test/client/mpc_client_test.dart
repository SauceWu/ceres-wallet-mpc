import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:ceres_mpc/src/bridge/mpc_engine.dart';
import 'package:ceres_mpc/src/client/mpc_client.dart';
import 'package:ceres_mpc/src/client/mpc_exceptions.dart';
import 'package:ceres_mpc/src/dto/mpc_dtos.dart';
import 'package:ceres_mpc/src/transport/mpc_transport.dart';

class MockMpcEngine extends Mock implements MpcEngine {}

class MockMpcTransport extends Mock implements MpcTransport {}

String _rpcOk(Object result, {int id = 1}) => jsonEncode({
      'jsonrpc': '2.0',
      'result': result,
      'id': id,
    });

String _rpcError(int code, String message, {int id = 1}) => jsonEncode({
      'jsonrpc': '2.0',
      'error': {'code': code, 'message': message},
      'id': id,
    });

void main() {
  late MockMpcEngine mockEngine;
  late MockMpcTransport mockTransport;
  late MpcClient client;

  setUp(() {
    mockEngine = MockMpcEngine();
    mockTransport = MockMpcTransport();
    client = MpcClient(engine: mockEngine, transport: mockTransport);
  });

  group('keygen', () {
    test('completes full keygen round-trip and returns KeygenResult', () async {
      // Transport: round 1 returns sessionId, round 2+ returns serverPayload
      when(() => mockTransport.send(any()))
          .thenAnswer((invocation) async {
        final payload = jsonDecode(invocation.positionalArguments[0] as String)
            as Map<String, dynamic>;
        final params = payload['params'] as Map<String, dynamic>;
        final round = params['round'] as int? ?? 1;

        if (round == 1) {
          return _rpcOk({
            'sessionId': 'sess_kg1',
            'serverPayload': {'round': 1, 'data': 'server_msg'},
          }, id: payload['id'] as int);
        } else {
          return _rpcOk({
            'serverPayload': {'round': round, 'data': 'server_msg'},
          }, id: payload['id'] as int);
        }
      });

      // Engine: round 1 → continue, round 2 → completed
      when(() => mockEngine.keygen('sess_kg1', 1, any()))
          .thenAnswer((_) async => const MpcRoundResult(
                status: 'continue', round: 1,
                clientPayload: 'client_round1_payload',
              ));

      when(() => mockEngine.keygen('sess_kg1', 2, any()))
          .thenAnswer((_) async => MpcRoundResult(
                status: 'completed', round: 2,
                clientPayload: jsonEncode({
                  'mpc_key_id': 'key_001',
                  'address': '0x1234567890abcdef1234567890abcdef12345678',
                  'public_key': '02abc123',
                  'curve': 'secp256k1',
                  'threshold': 2,
                  'key_ref': 'ref_001',
                  'backup_state': 'pending',
                  'rotation_version': 1,
                  'local_encrypted_share': 'encrypted_share_blob',
                }),
              ));

      final result = await client.keygen();

      expect(result.mpcKeyId, equals('key_001'));
      expect(result.address, equals('0x1234567890abcdef1234567890abcdef12345678'));
      expect(result.rotationVersion, equals(1));
      expect(result.localEncryptedShare, equals('encrypted_share_blob'));
    });

    test('throws MpcProtocolException when engine returns error', () async {
      when(() => mockTransport.send(any()))
          .thenAnswer((_) async => _rpcOk({
                'sessionId': 'sess_err',
                'serverPayload': {'round': 1},
              }));

      when(() => mockEngine.keygen('sess_err', 1, any()))
          .thenAnswer((_) async => const MpcRoundResult(
                status: 'error', round: 1,
                errorMessage: 'Protocol verification failed',
              ));

      expect(
        () => client.keygen(),
        throwsA(isA<MpcProtocolException>().having(
          (e) => e.message, 'message',
          contains('Protocol verification failed'),
        )),
      );
    });

    test('throws MpcTransportException when transport fails', () async {
      when(() => mockTransport.send(any()))
          .thenThrow(Exception('Network timeout'));

      expect(
        () => client.keygen(),
        throwsA(isA<MpcTransportException>().having(
          (e) => e.method, 'method', equals(MpcMethod.keygen.method),
        )),
      );
    });

    test('throws MpcRpcException when server returns JSON-RPC error', () async {
      when(() => mockTransport.send(any()))
          .thenAnswer((_) async =>
              _rpcError(MpcRpcException.sessionNotFound, 'Session expired'));

      expect(
        () => client.keygen(),
        throwsA(isA<MpcRpcException>()
            .having((e) => e.code, 'code', MpcRpcException.sessionNotFound)
            .having((e) => e.message, 'message', 'Session expired')),
      );
    });
  });

  group('recover', () {
    test('completes full recovery round-trip and returns RecoveryResult', () async {
      when(() => mockEngine.decryptBackupShare(any(), any()))
          .thenAnswer((_) async => 'decrypted_backup_share');

      when(() => mockTransport.send(any()))
          .thenAnswer((invocation) async {
        final payload = jsonDecode(invocation.positionalArguments[0] as String)
            as Map<String, dynamic>;
        final params = payload['params'] as Map<String, dynamic>;
        final round = params['round'] as int? ?? 1;

        if (round == 1) {
          return _rpcOk({
            'sessionId': 'sess_rc1',
            'serverPayload': {'round': 1},
          }, id: payload['id'] as int);
        } else {
          return _rpcOk({
            'serverPayload': {'round': round},
          }, id: payload['id'] as int);
        }
      });

      when(() => mockEngine.recover(
        'sess_rc1', 1, any(),
        backupShare: 'decrypted_backup_share',
        currentRotationVersion: 1,
      )).thenAnswer((_) async => const MpcRoundResult(
            status: 'continue', round: 1,
            clientPayload: 'client_rotation_payload',
          ));

      when(() => mockEngine.recover('sess_rc1', 2, any()))
          .thenAnswer((_) async => MpcRoundResult(
                status: 'completed', round: 2,
                clientPayload: jsonEncode({
                  'mpc_key_id': 'key_001',
                  'address': '0x1234567890abcdef1234567890abcdef12345678',
                  'public_key': '02abc123',
                  'rotation_version': 2,
                  'local_encrypted_share': 'new_encrypted_share',
                }),
              ));

      final result = await client.recover(
        mpcKeyId: 'key_001',
        encryptedBackupShare: 'encrypted_backup_blob',
        userBackupSecret: 'user_secret',
        currentRotationVersion: 1,
      );

      expect(result.mpcKeyId, equals('key_001'));
      expect(result.rotationVersion, equals(2));
      expect(result.localEncryptedShare, equals('new_encrypted_share'));
    });
  });

  group('sign', () {
    test('completes full sign round-trip when server returns completed', () async {
      when(() => mockTransport.send(any()))
          .thenAnswer((invocation) async {
        final payload = jsonDecode(invocation.positionalArguments[0] as String)
            as Map<String, dynamic>;
        final params = payload['params'] as Map<String, dynamic>;
        final round = params['round'] as int? ?? 1;

        if (round == 1) {
          return _rpcOk({
            'sessionId': 'sess_sg1',
            'serverPayload': {'message_hash': 'a' * 64},
          }, id: payload['id'] as int);
        } else {
          return _rpcOk({
            'status': 'completed',
            'r': 'aabb11',
            's': 'ccdd22',
            'recid': 0,
          }, id: payload['id'] as int);
        }
      });

      when(() => mockEngine.sign(
        'sess_sg1', 1, any(),
        share: 'my_share',
        messageHashHex: 'a' * 64,
      )).thenAnswer((_) async => const MpcRoundResult(
            status: 'continue', round: 1,
            clientPayload: 'client_eph_payload',
          ));

      final result = await client.sign(
        mpcKeyId: 'key_001',
        messageHash: 'a' * 64,
        localEncryptedShare: 'my_share',
      );

      expect(result.r, equals('aabb11'));
      expect(result.s, equals('ccdd22'));
      expect(result.recid, equals(0));
    });

    test('throws MpcProtocolException when sign returns error', () async {
      when(() => mockTransport.send(any()))
          .thenAnswer((_) async => _rpcOk({
                'sessionId': 'sess_err',
                'serverPayload': {'message_hash': 'a' * 64},
              }));

      when(() => mockEngine.sign(
        'sess_err', 1, any(),
        share: 'share',
        messageHashHex: 'a' * 64,
      )).thenAnswer((_) async => const MpcRoundResult(
            status: 'error', round: 1,
            errorMessage: 'Invalid share',
          ));

      expect(
        () => client.sign(
          mpcKeyId: 'key_001',
          messageHash: 'a' * 64,
          localEncryptedShare: 'share',
        ),
        throwsA(isA<MpcProtocolException>()),
      );
    });

    test('throws MpcTransportException when transport fails', () async {
      when(() => mockTransport.send(any()))
          .thenThrow(Exception('Network error'));

      expect(
        () => client.sign(
          mpcKeyId: 'key_001',
          messageHash: 'a' * 64,
          localEncryptedShare: 'share',
        ),
        throwsA(isA<MpcTransportException>().having(
          (e) => e.method, 'method', equals(MpcMethod.sign.method),
        )),
      );
    });
  });

  group('export', () {
    test('passes raw base64 server share to engine export', () async {
      when(() => mockTransport.send(any())).thenAnswer((invocation) async {
        final payload = jsonDecode(invocation.positionalArguments[0] as String)
            as Map<String, dynamic>;
        return _rpcOk({
          'serverSharePrivate': 'YWJjMTIzPT0=',
        }, id: payload['id'] as int);
      });

      when(() => mockEngine.exportPrivateKey('local_share', 'YWJjMTIzPT0='))
          .thenAnswer((_) async => jsonEncode({
                'private_key': 'deadbeef',
                'address': '0x1234567890abcdef1234567890abcdef12345678',
                'exported': true,
              }));

      final result = await client.exportPrivateKey(
        mpcKeyId: 'key_001',
        localEncryptedShare: 'local_share',
      );

      expect(result.privateKey, equals('deadbeef'));
      expect(result.exported, isTrue);
      verify(() => mockEngine.exportPrivateKey('local_share', 'YWJjMTIzPT0='))
          .called(1);
    });
  });
}
