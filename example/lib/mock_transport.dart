/// Full-fidelity mock transport simulating a JSON-RPC 2.0 MPC server.
///
/// Mimics real server behavior including:
/// - Session lifecycle (create → use → expire)
/// - Correct JSON field names matching kms-secp256k1 serde output
/// - Request validation (missing params, unknown session)
/// - Error responses using standard JSON-RPC error codes
///
/// Use this to:
/// - Run examples without a real backend
/// - Understand the full request/response structure
/// - Test host app integration end-to-end
library;

import 'dart:convert';
import 'dart:math';
import 'package:ceres_mpc/ceres_mpc.dart';

/// Simulates a complete MPC server (Party1) with session state management.
class MockMpcTransport implements MpcTransport {
  /// Active sessions keyed by sessionId.
  final Map<String, _MockSession> _sessions = {};

  /// Stored key shares keyed by mpcKeyId (simulates server DB).
  final Map<String, _MockKeyRecord> _keyStore = {};

  final _rand = Random(42); // deterministic for reproducibility

  @override
  Future<String> send(String payload) async {
    await Future.delayed(const Duration(milliseconds: 50));

    final Map<String, dynamic> request;
    try {
      request = jsonDecode(payload) as Map<String, dynamic>;
    } catch (_) {
      return _rpcError(-32700, 'Parse error', null, null);
    }

    final jsonrpc = request['jsonrpc'];
    final method = request['method'] as String?;
    final params = request['params'] as Map<String, dynamic>? ?? {};
    final id = request['id'];

    if (jsonrpc != '2.0' || method == null) {
      return _rpcError(-32600, 'Invalid request', null, id);
    }

    try {
      final ep = MpcMethod.values.where((e) => e.method == method).firstOrNull;
      final result = switch (ep) {
        MpcMethod.keygenStart => _keygenStart(params),
        MpcMethod.keygenContinue => _keygenContinue(params),
        MpcMethod.recoveryStart => _recoveryStart(params),
        MpcMethod.recoveryContinue => _recoveryContinue(params),
        MpcMethod.signStart => _signStart(params),
        MpcMethod.signContinue => _signContinue(params),
        MpcMethod.exportKey => _exportKey(params),
        null => throw _RpcError(-32601, 'Method not found: $method'),
      };
      return _rpcOk(result, id);
    } on _RpcError catch (e) {
      return _rpcError(e.code, e.message, null, id);
    }
  }

  // ── Keygen ──────────────────────────────────────────────────────

  Map<String, dynamic> _keygenStart(Map<String, dynamic> params) {
    final sessionId = _generateSessionId('kg');

    // Simulate Party1 keygen round 1:
    // MasterKey1::key_gen_first_message() produces commitment + zk proof
    // ChainCode1::chain_code_first_message() produces CC commitment
    final session = _MockSession(
      type: _SessionType.keygen,
      round: 1,
      // In real server, these would be actual cryptographic state
      state: {
        'kg_comm_witness_public_share': _mockHex(64),
        'cc_comm_witness_public_share': _mockHex(64),
        'kg_ec_key_pair_party1': _mockHex(32),
        'party_one_private': _mockHex(32),
      },
    );
    _sessions[sessionId] = session;

    return {
      'sessionId': sessionId,
      'serverPayload': {
        // KeyGenFirstMsg: Pedersen commitment to public key + ZK PoK of DLog
        'kg_party_one_first_message': {
          'pk_commitment': _mockHex(64),
          'zk_pok_commitment': _mockHex(64),
        },
        // ChainCode Party1FirstMessage: commitment for HD derivation chain code
        'cc_party_one_first_message': {
          'pk_commitment': _mockHex(64),
          'zk_pok_commitment': _mockHex(64),
        },
      },
    };
  }

  Map<String, dynamic> _keygenContinue(Map<String, dynamic> params) {
    final sessionId = params['sessionId'] as String?;
    if (sessionId == null) {
      throw _RpcError(-32600, 'Missing sessionId in params');
    }

    final session = _sessions.remove(sessionId);
    if (session == null || session.type != _SessionType.keygen) {
      throw _RpcError(-32001, 'Session not found or expired: $sessionId');
    }

    // Simulate Party1 keygen round 2:
    // Verify client's DLog proof, reveal commitment witness,
    // assemble MasterKey1 and persist it.

    // Generate a deterministic mock key pair for this session
    final mockMpcKeyId = 'mpc_${_mockHex(8)}';
    final mockPublicKeyHex = '04${_mockHex(128)}'; // uncompressed secp256k1
    final mockAddress = '0x${_mockHex(40)}';

    // Store the key in mock DB
    _keyStore[mockMpcKeyId] = _MockKeyRecord(
      mpcKeyId: mockMpcKeyId,
      address: mockAddress,
      publicKey: mockPublicKeyHex,
      masterKey1Serialized: _mockHex(256), // serialized MasterKey1
      rotationVersion: 1,
      exported: false,
    );

    return {
      'sessionId': sessionId,
      'serverPayload': {
        // KeyGenParty1Message2: revealed commitment + Paillier public key
        'kg_party_one_second_message': {
          'ecdh_second_message': {
            'comm_witness': {
              'public_share': session.state['kg_comm_witness_public_share'],
              'pk_commitment_blind_factor': _mockHex(64),
              'zk_pok_blind_factor': _mockHex(64),
            },
          },
          'ek': _mockHex(512), // Paillier encryption key
          'c_key': _mockHex(512), // encrypted Party1 secret
          'correct_key_proof': {
            'sigma_vec': List.generate(10, (_) => _mockHex(64)),
          },
          'range_proof': {
            'composite_dlog_proof': _mockHex(128),
          },
        },
        // ChainCode Party1SecondMessage: revealed CC commitment
        'cc_party_one_second_message': {
          'comm_witness': {
            'public_share': session.state['cc_comm_witness_public_share'],
            'pk_commitment_blind_factor': _mockHex(64),
            'zk_pok_blind_factor': _mockHex(64),
          },
        },
      },
    };
  }

  // ── Recovery ────────────────────────────────────────────────────

  Map<String, dynamic> _recoveryStart(Map<String, dynamic> params) {
    final mpcKeyId = params['mpcKeyId'] as String?;
    if (mpcKeyId == null) {
      throw _RpcError(-32600, 'Missing mpcKeyId in params');
    }

    final keyRecord = _keyStore[mpcKeyId];
    if (keyRecord == null) {
      throw _RpcError(-32003, 'Key not found: $mpcKeyId');
    }
    if (keyRecord.exported) {
      throw _RpcError(-32004, 'Key already exported: $mpcKeyId');
    }

    final sessionId = _generateSessionId('rc');

    // Simulate Rotation1::key_rotate_first_message()
    // Generates coin-flip commitment for random rotation factor
    _sessions[sessionId] = _MockSession(
      type: _SessionType.recovery,
      round: 1,
      state: {
        'mpcKeyId': mpcKeyId,
        'coin_flip_m1': _mockHex(64), // Party1's coin-flip secret
        'coin_flip_r1': _mockHex(64), // Party1's coin-flip randomness
      },
    );

    return {
      'sessionId': sessionId,
      'serverPayload': {
        // CoinFlip Party1FirstMessage: commitment to random rotation factor
        'coin_flip_party1_first_message': {
          'pk_commitment': _mockHex(64),
          'zk_pok_commitment': _mockHex(64),
        },
      },
    };
  }

  Map<String, dynamic> _recoveryContinue(Map<String, dynamic> params) {
    final sessionId = params['sessionId'] as String?;
    if (sessionId == null) {
      throw _RpcError(-32600, 'Missing sessionId in params');
    }

    final session = _sessions.remove(sessionId);
    if (session == null || session.type != _SessionType.recovery) {
      throw _RpcError(-32001, 'Session not found or expired: $sessionId');
    }

    final mpcKeyId = session.state['mpcKeyId'] as String;
    final keyRecord = _keyStore[mpcKeyId]!;

    // Simulate:
    // 1. Rotation1::key_rotate_second_message() — complete coin-flip
    // 2. master_key1.rotation_first_message() — apply rotation to MasterKey1
    // 3. Persist new MasterKey1

    keyRecord.masterKey1Serialized = _mockHex(256); // new rotated key
    keyRecord.rotationVersion += 1;

    return {
      'sessionId': sessionId,
      'serverPayload': {
        // CoinFlip Party1SecondMessage: reveal commitment
        'coin_flip_party1_second_message': {
          'comm_witness': {
            'public_share': _mockHex(64),
            'pk_commitment_blind_factor': _mockHex(64),
            'zk_pok_blind_factor': _mockHex(64),
          },
        },
        // RotationParty1Message1: rotation proof for Party2
        'rotation_party1_first_message': {
          'ek': _mockHex(512), // new Paillier encryption key
          'c_key_new': _mockHex(512), // new encrypted Party1 secret
          'correct_key_proof': {
            'sigma_vec': List.generate(10, (_) => _mockHex(64)),
          },
          'range_proof': {
            'composite_dlog_proof': _mockHex(128),
          },
        },
      },
    };
  }

  // ── Sign ────────────────────────────────────────────────────────

  Map<String, dynamic> _signStart(Map<String, dynamic> params) {
    final mpcKeyId = params['mpcKeyId'] as String?;
    final messageHash = params['messageHash'] as String?;

    if (mpcKeyId == null || messageHash == null) {
      throw _RpcError(-32600, 'Missing mpcKeyId or messageHash in params');
    }
    if (messageHash.length != 64) {
      throw _RpcError(-32600, 'messageHash must be 64 hex chars (32 bytes)');
    }

    final keyRecord = _keyStore[mpcKeyId];
    if (keyRecord == null) {
      throw _RpcError(-32003, 'Key not found: $mpcKeyId');
    }
    if (keyRecord.exported) {
      throw _RpcError(-32004, 'Key already exported: $mpcKeyId');
    }

    final sessionId = _generateSessionId('sg');

    // Simulate Party1 ephemeral key generation for signing:
    // MasterKey1::sign_first_message() generates ephemeral EC key pair
    _sessions[sessionId] = _MockSession(
      type: _SessionType.sign,
      round: 1,
      state: {
        'mpcKeyId': mpcKeyId,
        'messageHash': messageHash,
        'eph_ec_key_pair_party1': _mockHex(32),
      },
    );

    return {
      'sessionId': sessionId,
      'serverPayload': {
        // EphKeyGenFirstMsg: Party1's ephemeral public key commitment
        'eph_key_gen_first_message_party_one': {
          'pk_commitment': _mockHex(64),
          'zk_pok_commitment': _mockHex(64),
        },
        'message_hash': messageHash,
      },
    };
  }

  Map<String, dynamic> _signContinue(Map<String, dynamic> params) {
    final sessionId = params['sessionId'] as String?;
    if (sessionId == null) {
      throw _RpcError(-32600, 'Missing sessionId in params');
    }

    final session = _sessions.remove(sessionId);
    if (session == null || session.type != _SessionType.sign) {
      throw _RpcError(-32001, 'Session not found or expired: $sessionId');
    }

    // Simulate:
    // Parse client's SignMessage (partial signature from Party2)
    // master_key1.sign_second_message() — complete the 2-party signature
    // Return (r, s, recid)

    return {
      'status': 'completed',
      'r': _mockHex(64), // 32-byte r component
      's': _mockHex(64), // 32-byte s component
      'recid': _rand.nextInt(2), // recovery id: 0 or 1
    };
  }

  // ── Export Key ──────────────────────────────────────────────────

  Map<String, dynamic> _exportKey(Map<String, dynamic> params) {
    final mpcKeyId = params['mpcKeyId'] as String?;
    if (mpcKeyId == null) {
      throw _RpcError(-32600, 'Missing mpcKeyId in params');
    }

    final keyRecord = _keyStore[mpcKeyId];
    if (keyRecord == null) {
      throw _RpcError(-32003, 'Key not found: $mpcKeyId');
    }
    if (keyRecord.exported) {
      throw _RpcError(-32004, 'Key already exported: $mpcKeyId');
    }

    // Mark as exported — all future MPC operations will be rejected
    keyRecord.exported = true;

    // Return Party1's private share for key reconstruction
    // In real server: serde_json::to_value(&master_key1.private)
    return {
      'serverSharePrivate': {
        // Party1Private.x1: the secret scalar (FE)
        'x1': _mockHex(64),
        // Paillier private key components
        'paillier_priv': {
          'p': _mockHex(256),
          'q': _mockHex(256),
        },
        // Randomness used in Paillier encryption of x1
        'c_key_randomness': _mockHex(512),
      },
    };
  }

  // ── Helpers ─────────────────────────────────────────────────────

  String _generateSessionId(String prefix) {
    final timestamp = DateTime.now().millisecondsSinceEpoch;
    final random = _mockHex(8);
    return '${prefix}_${timestamp}_$random';
  }

  /// Generate deterministic mock hex string of given length.
  String _mockHex(int length) {
    const chars = '0123456789abcdef';
    return List.generate(length, (_) => chars[_rand.nextInt(16)]).join();
  }

  String _rpcOk(Object result, Object? id) => jsonEncode({
        'jsonrpc': '2.0',
        'result': result,
        'id': id,
      });

  String _rpcError(int code, String message, Object? data, Object? id) =>
      jsonEncode({
        'jsonrpc': '2.0',
        'error': {
          'code': code,
          'message': message,
          if (data != null) 'data': data,
        },
        'id': id,
      });
}

// ── Internal types ────────────────────────────────────────────────

enum _SessionType { keygen, recovery, sign }

class _MockSession {
  final _SessionType type;
  int round;
  final Map<String, dynamic> state;

  _MockSession({
    required this.type,
    required this.round,
    required this.state,
  });
}

class _MockKeyRecord {
  final String mpcKeyId;
  final String address;
  final String publicKey;
  String masterKey1Serialized;
  int rotationVersion;
  bool exported;

  _MockKeyRecord({
    required this.mpcKeyId,
    required this.address,
    required this.publicKey,
    required this.masterKey1Serialized,
    required this.rotationVersion,
    required this.exported,
  });
}

class _RpcError implements Exception {
  final int code;
  final String message;

  _RpcError(this.code, this.message);
}
