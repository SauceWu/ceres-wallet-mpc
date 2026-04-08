import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_mpc_wallet/flutter_mpc_wallet.dart';

void main() {
  group('KeygenResult.toString redaction', () {
    test('redacts localEncryptedShare and encryptedBackupShare', () {
      final result = KeygenResult(
        mpcKeyId: 'key_123',
        address: '0xABC',
        publicKey: '02abc',
        curve: 'secp256k1',
        threshold: 2,
        keyRef: 'ref_1',
        backupState: 'pending',
        rotationVersion: 1,
        localEncryptedShare: 'SUPER_SECRET_SHARE',
        encryptedBackupShare: 'SUPER_SECRET_BACKUP',
      );
      final s = result.toString();
      expect(s, contains('[REDACTED]'));
      expect(s, isNot(contains('SUPER_SECRET_SHARE')));
      expect(s, isNot(contains('SUPER_SECRET_BACKUP')));
      expect(s, contains('key_123'));
      expect(s, contains('0xABC'));
      expect(s, contains('02abc'));
      expect(s, contains('secp256k1'));
    });
  });

  group('RecoveryResult.toString redaction', () {
    test('redacts localEncryptedShare and encryptedBackupShare', () {
      final result = RecoveryResult(
        mpcKeyId: 'key_456',
        address: '0xDEF',
        publicKey: '03def',
        rotationVersion: 2,
        localEncryptedShare: 'RECOVERY_SECRET_SHARE',
        encryptedBackupShare: 'RECOVERY_SECRET_BACKUP',
      );
      final s = result.toString();
      expect(s, contains('[REDACTED]'));
      expect(s, isNot(contains('RECOVERY_SECRET_SHARE')));
      expect(s, isNot(contains('RECOVERY_SECRET_BACKUP')));
      expect(s, contains('key_456'));
      expect(s, contains('0xDEF'));
    });
  });

  group('BackupEnvelope', () {
    test('fromJson parses snake_case keys correctly', () {
      final json = {
        'version': '1',
        'algorithm': 'stub',
        'created_at': '1970-01-01T00:00:00Z',
        'payload': 'stub_envelope_abc',
      };
      final env = BackupEnvelope.fromJson(json);
      expect(env.version, '1');
      expect(env.algorithm, 'stub');
      expect(env.createdAt, '1970-01-01T00:00:00Z');
      expect(env.payload, 'stub_envelope_abc');
    });

    test('toString redacts payload but shows metadata', () {
      final env = BackupEnvelope(
        version: '1',
        algorithm: 'stub',
        createdAt: '1970-01-01T00:00:00Z',
        payload: 'SECRET_PAYLOAD_DATA',
      );
      final s = env.toString();
      expect(s, contains('[REDACTED]'));
      expect(s, isNot(contains('SECRET_PAYLOAD_DATA')));
      expect(s, contains('version: 1'));
      expect(s, contains('algorithm: stub'));
      expect(s, contains('createdAt: 1970-01-01T00:00:00Z'));
    });
  });

  group('Non-sensitive DTOs do NOT redact', () {
    test('MpcRoundResult.toString does not contain REDACTED', () {
      final r = MpcRoundResult(
        status: 'continue',
        round: 1,
        clientPayload: 'some_payload',
      );
      final s = r.toString();
      expect(s, isNot(contains('[REDACTED]')));
    });

    test('SignResult.toString does not contain REDACTED', () {
      final r = SignResult(
        signature: 'sig_abc',
        signedTx: 'tx_123',
        txHash: 'hash_456',
      );
      final s = r.toString();
      expect(s, isNot(contains('[REDACTED]')));
    });
  });
}
