/// Mock transport for testing and demonstration purposes.
///
/// Simulates server responses without a real backend. Useful for:
/// - Running examples without a server
/// - Unit testing host app integration
/// - Understanding the protocol message flow
library;

import 'dart:convert';
import 'package:ceres_mpc/ceres_mpc.dart';

/// A mock [MpcTransport] that returns canned server responses.
///
/// This demonstrates the expected request/response format for each endpoint.
/// In production, replace this with [HttpMpcTransport].
class MockMpcTransport implements MpcTransport {
  @override
  Future<String> send(String endpoint, String payload) async {
    // Simulate network latency
    await Future.delayed(const Duration(milliseconds: 100));

    final ep = MpcEndpoint.values.where((e) => e.path == endpoint).firstOrNull;
    return switch (ep) {
      MpcEndpoint.keygenStart => _keygenStart(),
      MpcEndpoint.keygenContinue => _keygenContinue(payload),
      MpcEndpoint.recoveryStart => _recoveryStart(payload),
      MpcEndpoint.recoveryContinue => _recoveryContinue(payload),
      MpcEndpoint.signStart => _signStart(payload),
      MpcEndpoint.signContinue => _signContinue(payload),
      MpcEndpoint.exportKey => _exportKey(payload),
      null => throw Exception('Unknown endpoint: $endpoint'),
    };
  }

  // ── Keygen ─────────────────────────────────────────────────────

  String _keygenStart() {
    // Server generates Party1 first messages and returns them.
    // In production, this contains real cryptographic commitments.
    return jsonEncode({
      'sessionId': 'keygen_session_001',
      'serverPayload': {
        'kg_party_one_first_message': {
          // Party1's keygen commitment
          'pk_commitment': 'mock_commitment_value',
          'zk_pok_commitment': 'mock_zk_commitment',
        },
        'cc_party_one_first_message': {
          // Party1's chain code commitment
          'pk_commitment': 'mock_cc_commitment',
          'zk_pok_commitment': 'mock_cc_zk_commitment',
        },
      },
    });
  }

  String _keygenContinue(String payload) {
    // Server verifies client's DLog proof, generates second messages,
    // assembles MasterKey1, and persists it.
    //
    // The client will use the response to assemble MasterKey2.
    return jsonEncode({
      'sessionId': 'keygen_session_001',
      'serverPayload': {
        'kg_party_one_second_message': {
          'ecdh_second_message': {
            'comm_witness': {
              'public_share': 'mock_public_share',
            },
          },
        },
        'cc_party_one_second_message': {
          'comm_witness': {
            'public_share': 'mock_cc_public_share',
          },
        },
      },
    });
  }

  // ── Recovery ───────────────────────────────────────────────────

  String _recoveryStart(String payload) {
    // Server loads existing MasterKey1 and starts coin-flip for rotation.
    return jsonEncode({
      'sessionId': 'recovery_session_001',
      'serverPayload': {
        'coin_flip_party1_first_message': {
          // Party1's coin-flip commitment
          'pk_commitment': 'mock_coin_flip_commitment',
          'zk_pok_commitment': 'mock_coin_flip_zk',
        },
      },
    });
  }

  String _recoveryContinue(String payload) {
    // Server completes coin-flip, applies rotation to MasterKey1,
    // and persists the new key.
    return jsonEncode({
      'sessionId': 'recovery_session_001',
      'serverPayload': {
        'coin_flip_party1_second_message': {
          'comm_witness': {
            'public_share': 'mock_coin_flip_reveal',
          },
        },
        'rotation_party1_first_message': {
          // Rotation message for Party2
          'ek': 'mock_rotation_ek',
          'c_key_new': 'mock_rotation_c_key',
        },
      },
    });
  }

  // ── Sign ───────────────────────────────────────────────────────

  String _signStart(String payload) {
    // Server generates ephemeral key for signing.
    return jsonEncode({
      'sessionId': 'sign_session_001',
      'serverPayload': {
        'eph_key_gen_first_message_party_one': {
          'pk_commitment': 'mock_eph_commitment',
          'zk_pok_commitment': 'mock_eph_zk',
        },
        'message_hash': (jsonDecode(payload) as Map)['messageHash'],
      },
    });
  }

  String _signContinue(String payload) {
    // Server completes signing, returns (r, s, recid).
    // Client uses these to assemble the final ECDSA signature.
    return jsonEncode({
      'status': 'completed',
      'r': 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
      's': 'f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5',
      'recid': 0,
    });
  }

  // ── Export Key ─────────────────────────────────────────────────────

  String _exportKey(String payload) {
    // Server authenticates the request (strong auth required!),
    // then returns Party1's private share for key reconstruction.
    //
    // SECURITY: In production, this endpoint requires:
    // - Multi-factor authentication
    // - Rate limiting
    // - Audit logging
    // - The server should mark the key as "exported" after this
    //
    // After export, the MPC key pair should be considered compromised
    // and no longer used for MPC operations.
    return jsonEncode({
      'serverSharePrivate': {
        // This is Party1's private key material (x1).
        // In production, this is the real serialized Party1Private.
        'x1': 'mock_party1_secret_scalar',
        'paillier_priv': 'mock_paillier_private_key',
        'c_key_randomness': 'mock_randomness',
      },
    });
  }
}
