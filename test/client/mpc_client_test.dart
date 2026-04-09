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

/// Wrap data in a JSON-RPC 2.0 success response.
String _rpcOk(Object result, {int id = 1}) => jsonEncode({
      'jsonrpc': '2.0',
      'result': result,
      'id': id,
    });

/// Wrap error in a JSON-RPC 2.0 error response.
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
      // First RPC call: keygen_start
      when(() => mockTransport.send(any()))
          .thenAnswer((invocation) async {
        final payload = jsonDecode(invocation.positionalArguments[0] as String)
            as Map<String, dynamic>;
        final method = payload['method'] as String;

        if (method == MpcMethod.keygenStart.method) {
          return _rpcOk({
            'sessionId': 'sess_kg1',
            'serverPayload': {'round': 1, 'data': 'server_first_msg'},
          }, id: payload['id'] as int);
        } else if (method == MpcMethod.keygenContinue.method) {
          return _rpcOk({
            'serverPayload': {'round': 2, 'data': 'server_second_msg'},
          }, id: payload['id'] as int);
        }
        throw Exception('Unexpected method: $method');
      });

      when(() => mockEngine.keygenStart('sess_kg1', any()))
          .thenAnswer((_) async => const MpcRoundResult(
                status: 'continue',
                round: 1,
                clientPayload: 'client_round1_payload',
              ));

      when(() => mockEngine.keygenContinue('sess_kg1', any()))
          .thenAnswer((_) async => MpcRoundResult(
                status: 'completed',
                round: 2,
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
      expect(result.address,
          equals('0x1234567890abcdef1234567890abcdef12345678'));
      expect(result.publicKey, equals('02abc123'));
      expect(result.curve, equals('secp256k1'));
      expect(result.threshold, equals(2));
      expect(result.rotationVersion, equals(1));
      expect(result.localEncryptedShare, equals('encrypted_share_blob'));
    });

    test('throws MpcProtocolException when engine returns error', () async {
      when(() => mockTransport.send(any()))
          .thenAnswer((_) async => _rpcOk({
                'sessionId': 'sess_err',
                'serverPayload': {'round': 1},
              }));

      when(() => mockEngine.keygenStart('sess_err', any()))
          .thenAnswer((_) async => const MpcRoundResult(
                status: 'error',
                round: 1,
                errorMessage: 'Protocol verification failed',
              ));

      expect(
        () => client.keygen(),
        throwsA(isA<MpcProtocolException>().having(
          (e) => e.message,
          'message',
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
          (e) => e.method,
          'method',
          equals(MpcMethod.keygenStart.method),
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
    test('completes full recovery round-trip and returns RecoveryResult',
        () async {
      when(() => mockEngine.decryptBackupShare(any(), any()))
          .thenAnswer((_) async => 'decrypted_backup_share');

      when(() => mockTransport.send(any()))
          .thenAnswer((invocation) async {
        final payload = jsonDecode(invocation.positionalArguments[0] as String)
            as Map<String, dynamic>;
        final method = payload['method'] as String;

        if (method == MpcMethod.recoveryStart.method) {
          return _rpcOk({
            'sessionId': 'sess_rc1',
            'serverPayload': {'round': 1, 'data': 'server_rotation_msg'},
          }, id: payload['id'] as int);
        } else if (method == MpcMethod.recoveryContinue.method) {
          return _rpcOk({
            'serverPayload': {'round': 2, 'data': 'server_rotation_msg2'},
          }, id: payload['id'] as int);
        }
        throw Exception('Unexpected method: $method');
      });

      when(() =>
              mockEngine.recoverStart('sess_rc1', 'decrypted_backup_share', any(), any()))
          .thenAnswer((_) async => const MpcRoundResult(
                status: 'continue',
                round: 1,
                clientPayload: 'client_rotation_payload',
              ));

      when(() => mockEngine.recoverContinue('sess_rc1', any()))
          .thenAnswer((_) async => MpcRoundResult(
                status: 'completed',
                round: 2,
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
      expect(result.address,
          equals('0x1234567890abcdef1234567890abcdef12345678'));
      expect(result.rotationVersion, equals(2));
      expect(result.localEncryptedShare, equals('new_encrypted_share'));

      verify(() => mockEngine.decryptBackupShare(
          'encrypted_backup_blob', 'user_secret')).called(1);
    });

    test('throws MpcProtocolException when recover engine returns error',
        () async {
      when(() => mockEngine.decryptBackupShare(any(), any()))
          .thenAnswer((_) async => 'backup_share');
      when(() => mockTransport.send(any()))
          .thenAnswer((_) async => _rpcOk({
                'sessionId': 'sess_err',
                'serverPayload': {'round': 1},
              }));
      when(() => mockEngine.recoverStart('sess_err', 'backup_share', any(), any()))
          .thenAnswer((_) async => const MpcRoundResult(
                status: 'error',
                round: 1,
                errorMessage: 'Recovery verification failed',
              ));

      expect(
        () => client.recover(
          mpcKeyId: 'key_001',
          encryptedBackupShare: 'enc',
          userBackupSecret: 'sec',
          currentRotationVersion: 1,
        ),
        throwsA(isA<MpcProtocolException>()),
      );
    });
  });

  group('sign', () {
    test('completes full sign round-trip when server returns completed',
        () async {
      when(() => mockTransport.send(any()))
          .thenAnswer((invocation) async {
        final payload = jsonDecode(invocation.positionalArguments[0] as String)
            as Map<String, dynamic>;
        final method = payload['method'] as String;

        if (method == MpcMethod.signStart.method) {
          return _rpcOk({
            'sessionId': 'sess_sg1',
            'serverPayload': {
              'eph_key_gen_first_message_party_one': {},
              'message_hash': 'a' * 64,
            },
          }, id: payload['id'] as int);
        } else if (method == MpcMethod.signContinue.method) {
          return _rpcOk({
            'status': 'completed',
            'r': 'aabb11',
            's': 'ccdd22',
            'recid': 0,
          }, id: payload['id'] as int);
        }
        throw Exception('Unexpected method: $method');
      });

      when(() => mockEngine.signStart('sess_sg1', 'my_share', any(), any()))
          .thenAnswer((_) async => const MpcRoundResult(
                status: 'continue',
                round: 1,
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

    test('throws MpcProtocolException when signStart returns error', () async {
      when(() => mockTransport.send(any()))
          .thenAnswer((_) async => _rpcOk({
                'sessionId': 'sess_err',
                'serverPayload': {'message_hash': 'a' * 64},
              }));

      when(() => mockEngine.signStart('sess_err', 'share', any(), any()))
          .thenAnswer((_) async => const MpcRoundResult(
                status: 'error',
                round: 1,
                errorMessage: 'Invalid share',
              ));

      expect(
        () => client.sign(
          mpcKeyId: 'key_001',
          messageHash: 'a' * 64,
          localEncryptedShare: 'share',
        ),
        throwsA(isA<MpcProtocolException>().having(
          (e) => e.message,
          'message',
          contains('Invalid share'),
        )),
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
          (e) => e.method,
          'method',
          equals(MpcMethod.signStart.method),
        )),
      );
    });

    test('full round-trip with Rust signContinue before server completes',
        () async {
      when(() => mockTransport.send(any()))
          .thenAnswer((invocation) async {
        final payload = jsonDecode(invocation.positionalArguments[0] as String)
            as Map<String, dynamic>;
        final method = payload['method'] as String;

        if (method == MpcMethod.signStart.method) {
          return _rpcOk({
            'sessionId': 'sess_sg2',
            'serverPayload': {'message_hash': 'b' * 64},
          }, id: payload['id'] as int);
        } else if (method == MpcMethod.signContinue.method) {
          return _rpcOk({
            'status': 'completed',
            'r': 'ff00',
            's': '1122',
            'recid': 1,
          }, id: payload['id'] as int);
        }
        throw Exception('Unexpected method: $method');
      });

      when(() => mockEngine.signStart('sess_sg2', 'share2', any(), any()))
          .thenAnswer((_) async => const MpcRoundResult(
                status: 'continue',
                round: 1,
                clientPayload: 'eph_payload',
              ));

      final result = await client.sign(
        mpcKeyId: 'key_002',
        messageHash: 'b' * 64,
        localEncryptedShare: 'share2',
      );

      expect(result.r, equals('ff00'));
      expect(result.s, equals('1122'));
      expect(result.recid, equals(1));
    });
  });
}
