/// Full-fidelity mock transport simulating a JSON-RPC 2.0 MPC server.
///
/// Mimics real server behavior including:
/// - Session lifecycle (create → use → expire)
/// - WireEnvelope format matching sl-dkls23 ChannelRelay protocol
/// - Multi-round protocol (4 rounds for keygen/sign/recovery)
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
///
/// The server uses WireEnvelope format for all protocol messages:
/// {session_id, protocol, round, from_id, to_id, payload_encoding, payload, step?}
///
/// Protocol rounds (DKLs23 4-round):
/// - keygen_start: returns round 1 WireEnvelope
/// - keygen_continue (round 2): returns round 2 WireEnvelope
/// - keygen_continue (round 3): returns round 3 WireEnvelope
/// - keygen_continue (round 4): returns completed result (no more WireEnvelope)
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

  // ── Keygen (4-round DKLs23 DKG) ────────────────────────────────

  Map<String, dynamic> _keygenStart(Map<String, dynamic> params) {
    final sessionId = _generateSessionId('kg');

    // Server DKG round 1: generate initial protocol message via Relay
    _sessions[sessionId] = _MockSession(
      type: _SessionType.keygen,
      round: 1,
      maxRounds: 4,
      state: {},
    );

    return {
      'sessionId': sessionId,
      'serverPayload': _wireEnvelope(
        sessionId: sessionId,
        protocol: 'dkg',
        round: 1,
        fromId: 1,
        toId: 0,
      ),
    };
  }

  Map<String, dynamic> _keygenContinue(Map<String, dynamic> params) {
    final sessionId = params['sessionId'] as String?;
    if (sessionId == null) {
      throw _RpcError(-32600, 'Missing sessionId in params');
    }

    final session = _sessions[sessionId];
    if (session == null || session.type != _SessionType.keygen) {
      throw _RpcError(-32001, 'Session not found or expired: $sessionId');
    }

    session.round += 1;

    // Round 2 and 3: return next WireEnvelope
    if (session.round < session.maxRounds) {
      return {
        'sessionId': sessionId,
        'serverPayload': _wireEnvelope(
          sessionId: sessionId,
          protocol: 'dkg',
          round: session.round,
          fromId: 1,
          toId: 0,
        ),
      };
    }

    // Round 4 (final): protocol complete, return KeygenResult
    _sessions.remove(sessionId);

    final mockMpcKeyId = 'mpc_${_mockHex(8)}';
    final mockPublicKeyHex = '04${_mockHex(128)}'; // uncompressed secp256k1
    final mockAddress = '0x${_mockHex(40)}';

    _keyStore[mockMpcKeyId] = _MockKeyRecord(
      mpcKeyId: mockMpcKeyId,
      address: mockAddress,
      publicKey: mockPublicKeyHex,
      keyshareSerialized: _mockHex(256),
      rotationVersion: 1,
      exported: false,
    );

    return {
      'status': 'completed',
      'mpcKeyId': mockMpcKeyId,
      'address': mockAddress,
      'publicKey': mockPublicKeyHex,
      'curve': 'secp256k1',
      'threshold': 2,
      'keyRef': mockMpcKeyId,
      'backupState': 'pending',
      'rotationVersion': 1,
      'localEncryptedShare': base64Encode(List.generate(128, (_) => _rand.nextInt(256))),
    };
  }

  // ── Recovery (4-round DKLs23 key_refresh) ──────────────────────

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

    _sessions[sessionId] = _MockSession(
      type: _SessionType.recovery,
      round: 1,
      maxRounds: 4,
      state: {'mpcKeyId': mpcKeyId},
    );

    return {
      'sessionId': sessionId,
      'serverPayload': _wireEnvelope(
        sessionId: sessionId,
        protocol: 'rotation',
        round: 1,
        fromId: 1,
        toId: 0,
      ),
    };
  }

  Map<String, dynamic> _recoveryContinue(Map<String, dynamic> params) {
    final sessionId = params['sessionId'] as String?;
    if (sessionId == null) {
      throw _RpcError(-32600, 'Missing sessionId in params');
    }

    final session = _sessions[sessionId];
    if (session == null || session.type != _SessionType.recovery) {
      throw _RpcError(-32001, 'Session not found or expired: $sessionId');
    }

    session.round += 1;

    if (session.round < session.maxRounds) {
      return {
        'sessionId': sessionId,
        'serverPayload': _wireEnvelope(
          sessionId: sessionId,
          protocol: 'rotation',
          round: session.round,
          fromId: 1,
          toId: 0,
        ),
      };
    }

    // Final round: complete recovery
    _sessions.remove(sessionId);

    final mpcKeyId = session.state['mpcKeyId'] as String;
    final keyRecord = _keyStore[mpcKeyId]!;

    keyRecord.keyshareSerialized = _mockHex(256); // new rotated key
    keyRecord.rotationVersion += 1;

    return {
      'status': 'completed',
      'mpcKeyId': mpcKeyId,
      'address': keyRecord.address,
      'publicKey': keyRecord.publicKey,
      'curve': 'secp256k1',
      'threshold': 2,
      'keyRef': mpcKeyId,
      'backupState': 'pending',
      'rotationVersion': keyRecord.rotationVersion,
      'localEncryptedShare': base64Encode(List.generate(128, (_) => _rand.nextInt(256))),
      'encryptedBackupShare': null,
    };
  }

  // ── Sign (4-round DKLs23 DSG) ─────────────────────────────────

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

    _sessions[sessionId] = _MockSession(
      type: _SessionType.sign,
      round: 1,
      maxRounds: 4,
      state: {
        'mpcKeyId': mpcKeyId,
        'messageHash': messageHash,
      },
    );

    return {
      'sessionId': sessionId,
      'serverPayload': _wireEnvelope(
        sessionId: sessionId,
        protocol: 'dsg',
        round: 1,
        fromId: 1,
        toId: 0,
      ),
    };
  }

  Map<String, dynamic> _signContinue(Map<String, dynamic> params) {
    final sessionId = params['sessionId'] as String?;
    if (sessionId == null) {
      throw _RpcError(-32600, 'Missing sessionId in params');
    }

    final session = _sessions[sessionId];
    if (session == null || session.type != _SessionType.sign) {
      throw _RpcError(-32001, 'Session not found or expired: $sessionId');
    }

    session.round += 1;

    if (session.round < session.maxRounds) {
      return {
        'sessionId': sessionId,
        'serverPayload': _wireEnvelope(
          sessionId: sessionId,
          protocol: 'dsg',
          round: session.round,
          fromId: 1,
          toId: 0,
        ),
      };
    }

    // Final round: return signature
    _sessions.remove(sessionId);

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

    keyRecord.exported = true;

    // Return server's keyshare as Base64 bytes for client-side reconstruction
    return {
      'serverKeyshare': base64Encode(List.generate(128, (_) => _rand.nextInt(256))),
    };
  }

  // ── Helpers ─────────────────────────────────────────────────────

  /// Build a WireEnvelope JSON map matching the Rust WireEnvelope struct.
  ///
  /// Fields: session_id, protocol, round, from_id, to_id,
  ///         payload_encoding ("cbor_base64"), payload (Base64 mock bytes)
  Map<String, dynamic> _wireEnvelope({
    required String sessionId,
    required String protocol,
    required int round,
    required int fromId,
    required int toId,
  }) {
    // Generate mock protocol bytes (simulates CBOR-encoded DKLs23 message)
    final mockPayloadBytes = List.generate(64 + _rand.nextInt(64), (_) => _rand.nextInt(256));
    final payloadBase64 = base64Encode(mockPayloadBytes);

    return {
      'session_id': sessionId,
      'protocol': protocol,
      'round': round,
      'from_id': fromId,
      'to_id': toId,
      'payload_encoding': 'cbor_base64',
      'payload': payloadBase64,
    };
  }

  String _generateSessionId(String prefix) {
    // Generate a 64-char hex string (32 bytes) as required by Rust InstanceId
    return _mockHex(64);
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
  final int maxRounds;
  final Map<String, dynamic> state;

  _MockSession({
    required this.type,
    required this.round,
    required this.maxRounds,
    required this.state,
  });
}

class _MockKeyRecord {
  final String mpcKeyId;
  final String address;
  final String publicKey;
  String keyshareSerialized;
  int rotationVersion;
  bool exported;

  _MockKeyRecord({
    required this.mpcKeyId,
    required this.address,
    required this.publicKey,
    required this.keyshareSerialized,
    required this.rotationVersion,
    required this.exported,
  });
}

class _RpcError implements Exception {
  final int code;
  final String message;

  _RpcError(this.code, this.message);
}
