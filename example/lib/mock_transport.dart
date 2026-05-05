/// Full-fidelity mock transport simulating a JSON-RPC 2.0 MPC server.
///
/// Mimics real server behavior including:
/// - Session lifecycle (create → use → expire)
/// - WireEnvelope format matching sl-dkls23 ChannelRelay protocol
/// - Multi-round protocol (4 rounds for EVM keygen/sign/recovery, 3 for SOL keygen/recovery, 2 for SOL sign)
/// - Unified method names: keygen, sign, recovery (round in params)
/// - Request validation (missing params, unknown session)
/// - Error responses using standard JSON-RPC error codes
library;

import 'dart:convert';
import 'dart:math';
import 'package:ceres_mpc/ceres_mpc.dart';

/// Simulates a complete MPC server (Party1) with session state management.
class MockMpcTransport implements MpcTransport {
  final Map<String, _MockSession> _sessions = {};
  final Map<String, _MockKeyRecord> _keyStore = {};
  final _rand = Random(42);

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
        MpcMethod.keygen => _keygen(params),
        MpcMethod.recovery => _recovery(params),
        MpcMethod.sign => _sign(params),
        MpcMethod.exportKey => _exportKey(params),
        null => throw _RpcError(-32601, 'Method not found: $method'),
      };
      return _rpcOk(result, id);
    } on _RpcError catch (e) {
      return _rpcError(e.code, e.message, null, id);
    }
  }

  // ── Keygen (unified, round-based) ─────────────────────────────

  Map<String, dynamic> _keygen(Map<String, dynamic> params) {
    final round = params['round'] as int? ?? 1;

    if (round == 1) {
      final isSol = params['curve'] == 'ed25519';
      final sessionId = _generateSessionId();
      _sessions[sessionId] = _MockSession(
        type: _SessionType.keygen,
        round: 1,
        maxRounds: isSol ? 3 : 4,
        state: {},
        isSol: isSol,
      );
      return {
        'sessionId': sessionId,
        'serverPayload': _wireEnvelope(sessionId, 'dkg', 1,
            curve: isSol ? 'ed25519' : null),
      };
    }

    final sessionId = params['sessionId'] as String?;
    if (sessionId == null) throw _RpcError(-32600, 'Missing sessionId');

    final session = _sessions[sessionId];
    if (session == null || session.type != _SessionType.keygen) {
      throw _RpcError(-32001, 'Session not found: $sessionId');
    }

    session.round += 1;

    if (session.round < session.maxRounds) {
      return {
        'sessionId': sessionId,
        'serverPayload': _wireEnvelope(sessionId, 'dkg', session.round,
            curve: session.isSol ? 'ed25519' : null),
      };
    }

    // Final round
    _sessions.remove(sessionId);
    final mockId = 'mpc_${_mockHex(8)}';

    if (session.isSol) {
      final mockAddr = _mockBase58Address();
      final mockPk = _mockHex(64);
      _keyStore[mockId] = _MockKeyRecord(
        mpcKeyId: mockId, address: mockAddr, publicKey: mockPk,
        keyshareSerialized: _mockHex(256), rotationVersion: 1, exported: false,
        curve: 'ed25519',
      );
      return {
        'status': 'completed',
        'mpcKeyId': mockId,
        'address': mockAddr,
        'publicKey': mockPk,
        'curve': 'ed25519',
        'threshold': 2,
        'keyRef': mockId,
        'backupState': 'pending',
        'rotationVersion': 1,
        'localEncryptedShare': _mockShareEnvelope(),
      };
    } else {
      final mockPk = '04${_mockHex(128)}';
      final mockAddr = '0x${_mockHex(40)}';
      _keyStore[mockId] = _MockKeyRecord(
        mpcKeyId: mockId, address: mockAddr, publicKey: mockPk,
        keyshareSerialized: _mockHex(256), rotationVersion: 1, exported: false,
        curve: 'secp256k1',
      );
      return {
        'status': 'completed',
        'mpcKeyId': mockId,
        'address': mockAddr,
        'publicKey': mockPk,
        'curve': 'secp256k1',
        'threshold': 2,
        'keyRef': mockId,
        'backupState': 'pending',
        'rotationVersion': 1,
        'localEncryptedShare': base64Encode(List.generate(128, (_) => _rand.nextInt(256))),
      };
    }
  }

  // ── Recovery (unified) ────────────────────────────────────────

  Map<String, dynamic> _recovery(Map<String, dynamic> params) {
    final round = params['round'] as int? ?? 1;

    if (round == 1) {
      final mpcKeyId = params['mpcKeyId'] as String?;
      if (mpcKeyId == null) throw _RpcError(-32600, 'Missing mpcKeyId');
      final kr = _keyStore[mpcKeyId];
      if (kr == null) throw _RpcError(-32003, 'Key not found: $mpcKeyId');
      if (kr.exported) throw _RpcError(-32004, 'Key already exported: $mpcKeyId');

      final isSol = kr.curve == 'ed25519';
      final sessionId = _generateSessionId();
      _sessions[sessionId] = _MockSession(
        type: _SessionType.recovery, round: 1, maxRounds: isSol ? 3 : 4,
        state: {'mpcKeyId': mpcKeyId},
        isSol: isSol,
      );
      return {
        'sessionId': sessionId,
        'serverPayload': _wireEnvelope(sessionId, 'rotation', 1,
            curve: isSol ? 'ed25519' : null),
      };
    }

    final sessionId = params['sessionId'] as String?;
    if (sessionId == null) throw _RpcError(-32600, 'Missing sessionId');
    final session = _sessions[sessionId];
    if (session == null || session.type != _SessionType.recovery) {
      throw _RpcError(-32001, 'Session not found: $sessionId');
    }

    session.round += 1;

    if (session.round < session.maxRounds) {
      return {
        'sessionId': sessionId,
        'serverPayload': _wireEnvelope(sessionId, 'rotation', session.round,
            curve: session.isSol ? 'ed25519' : null),
      };
    }

    _sessions.remove(sessionId);
    final mpcKeyId = session.state['mpcKeyId'] as String;
    final kr = _keyStore[mpcKeyId]!;
    kr.rotationVersion += 1;

    if (session.isSol) {
      kr.keyshareSerialized = _mockHex(256);
      return {
        'status': 'completed',
        'mpcKeyId': mpcKeyId,
        'address': kr.address,
        'publicKey': kr.publicKey,
        'curve': 'ed25519',
        'threshold': 2,
        'keyRef': mpcKeyId,
        'backupState': 'pending',
        'rotationVersion': kr.rotationVersion,
        'localEncryptedShare': _mockShareEnvelope(),
        'encryptedBackupShare': null,
      };
    } else {
      kr.keyshareSerialized = _mockHex(256);
      return {
        'status': 'completed',
        'mpcKeyId': mpcKeyId,
        'address': kr.address,
        'publicKey': kr.publicKey,
        'curve': 'secp256k1',
        'threshold': 2,
        'keyRef': mpcKeyId,
        'backupState': 'pending',
        'rotationVersion': kr.rotationVersion,
        'localEncryptedShare': base64Encode(List.generate(128, (_) => _rand.nextInt(256))),
        'encryptedBackupShare': null,
      };
    }
  }

  // ── Sign (unified) ────────────────────────────────────────────

  Map<String, dynamic> _sign(Map<String, dynamic> params) {
    final round = params['round'] as int? ?? 1;

    if (round == 1) {
      final mpcKeyId = params['mpcKeyId'] as String?;
      final messageHash = params['messageHash'] as String?;
      if (mpcKeyId == null || messageHash == null) {
        throw _RpcError(-32600, 'Missing mpcKeyId or messageHash');
      }
      if (messageHash.length != 64) {
        throw _RpcError(-32600, 'messageHash must be 64 hex chars');
      }
      final kr = _keyStore[mpcKeyId];
      if (kr == null) throw _RpcError(-32003, 'Key not found: $mpcKeyId');
      if (kr.exported) throw _RpcError(-32004, 'Key already exported: $mpcKeyId');

      final isSol = kr.curve == 'ed25519';
      final sessionId = _generateSessionId();
      _sessions[sessionId] = _MockSession(
        type: _SessionType.sign, round: 1, maxRounds: isSol ? 2 : 4,
        state: {'mpcKeyId': mpcKeyId, 'messageHash': messageHash},
        isSol: isSol,
      );
      return {
        'sessionId': sessionId,
        'serverPayload': _wireEnvelope(sessionId, 'dsg', 1,
            curve: isSol ? 'ed25519' : null),
      };
    }

    final sessionId = params['sessionId'] as String?;
    if (sessionId == null) throw _RpcError(-32600, 'Missing sessionId');
    final session = _sessions[sessionId];
    if (session == null || session.type != _SessionType.sign) {
      throw _RpcError(-32001, 'Session not found: $sessionId');
    }

    session.round += 1;

    if (session.round < session.maxRounds) {
      return {
        'sessionId': sessionId,
        'serverPayload': _wireEnvelope(sessionId, 'dsg', session.round,
            curve: session.isSol ? 'ed25519' : null),
      };
    }

    _sessions.remove(sessionId);

    if (session.isSol) {
      // FROST sign: return serverPayload for engine to aggregate (NOT "completed").
      // The engine processes round 2 and returns isCompleted=true with the signature.
      return {
        'sessionId': sessionId,
        'serverPayload': _wireEnvelope(sessionId, 'dsg', session.round,
            curve: 'ed25519'),
      };
    }

    // EVM: server returns completed result directly
    return {
      'status': 'completed',
      'r': _mockHex(64),
      's': _mockHex(64),
      'recid': _rand.nextInt(2),
    };
  }

  // ── Export Key ──────────────────────────────────────────────────

  Map<String, dynamic> _exportKey(Map<String, dynamic> params) {
    final mpcKeyId = params['mpcKeyId'] as String?;
    if (mpcKeyId == null) throw _RpcError(-32600, 'Missing mpcKeyId');
    final kr = _keyStore[mpcKeyId];
    if (kr == null) throw _RpcError(-32003, 'Key not found: $mpcKeyId');
    if (kr.exported) throw _RpcError(-32004, 'Key already exported: $mpcKeyId');
    kr.exported = true;
    return {'serverKeyshare': base64Encode(List.generate(128, (_) => _rand.nextInt(256)))};
  }

  // ── Helpers ─────────────────────────────────────────────────────

  Map<String, dynamic> _wireEnvelope(String sessionId, String protocol, int round, {String? curve}) {
    final mockBytes = List.generate(64 + _rand.nextInt(64), (_) => _rand.nextInt(256));
    return {
      'session_id': sessionId,
      'protocol': protocol,
      'round': round,
      'from_id': 1,
      'to_id': 0,
      'payload_encoding': 'cbor_base64',
      'payload': base64Encode(mockBytes),
      if (curve != null) 'curve': curve,
    };
  }

  String _generateSessionId() => _mockHex(64);

  String _mockHex(int length) {
    const chars = '0123456789abcdef';
    return List.generate(length, (_) => chars[_rand.nextInt(16)]).join();
  }

  /// Returns a 44-character mock Solana base58 address.
  String _mockBase58Address() {
    const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
    return List.generate(44, (_) => alphabet[_rand.nextInt(alphabet.length)]).join();
  }

  /// Returns a SharedEnvelope v2 JSON string for ed25519 key shares.
  String _mockShareEnvelope() {
    return jsonEncode({
      'v': 2,
      'curve': 'ed25519',
      'share': base64Encode(List.generate(32, (_) => _rand.nextInt(256))),
    });
  }

  String _rpcOk(Object result, Object? id) => jsonEncode({
    'jsonrpc': '2.0', 'result': result, 'id': id,
  });

  String _rpcError(int code, String message, Object? data, Object? id) => jsonEncode({
    'jsonrpc': '2.0',
    'error': {'code': code, 'message': message, if (data != null) 'data': data},
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
  final bool isSol;
  _MockSession({required this.type, required this.round, required this.maxRounds, required this.state, this.isSol = false});
}

class _MockKeyRecord {
  final String mpcKeyId, address, publicKey;
  String keyshareSerialized;
  int rotationVersion;
  bool exported;
  String curve;
  _MockKeyRecord({required this.mpcKeyId, required this.address, required this.publicKey,
    required this.keyshareSerialized, required this.rotationVersion, required this.exported,
    this.curve = 'secp256k1'});
}

class _RpcError implements Exception {
  final int code;
  final String message;
  _RpcError(this.code, this.message);
}
