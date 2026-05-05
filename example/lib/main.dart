/// ceres_mpc SDK usage examples.
///
/// Demonstrates keygen, recovery, sign, backup, and error handling flows.
/// Uses [MockMpcTransport] to run without a real server.
///
/// For production usage with a real server, see [http_transport_example.dart].
// ignore_for_file: implementation_imports, invalid_use_of_internal_member
library;

import 'dart:async';

import 'package:flutter/material.dart' hide Curve;
import 'package:ceres_mpc/ceres_mpc.dart';
import 'package:ceres_mpc/src/bridge/mpc_engine.dart';
import 'package:ceres_mpc/src/rust/frb_generated.dart';

import 'http_transport_example.dart';
import 'mock_engine.dart';
import 'mock_transport.dart';
import 'ws_transport_example.dart';

/// Set to true to use mock engine (no Rust crypto, for UI testing).
/// Set to false to use real Rust engine (requires real server).
const _useMockEngine = false;

enum ExampleTransportMode {
  mock('Mock', 'Local demo transport with no backend required.'),
  http('HTTP', 'Classic JSON-RPC over HTTP. Requires a real MPC backend.'),
  websocket('WebSocket', 'Persistent JSON-RPC over WebSocket with reconnect support.');

  const ExampleTransportMode(this.label, this.description);

  final String label;
  final String description;
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  if (!_useMockEngine) {
    await RustLib.init();
  }
  runApp(const ExampleApp());
}

class ExampleApp extends StatelessWidget {
  const ExampleApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'ceres_mpc Example',
      theme: ThemeData(primarySwatch: Colors.blue, useMaterial3: true),
      home: const ExampleHomePage(),
    );
  }
}

class ExampleHomePage extends StatefulWidget {
  const ExampleHomePage({super.key});

  @override
  State<ExampleHomePage> createState() => _ExampleHomePageState();
}

class _ExampleHomePageState extends State<ExampleHomePage> {
  final _logs = <String>[];
  late MpcClient _client;
  late MpcEngine _engine;
  late MpcTransport _transport;
  final _httpUrlController = TextEditingController(text: 'http://127.0.0.1:3000/rpc');
  final _wsUrlController = TextEditingController(text: 'ws://127.0.0.1:3000/ws');
  ExampleTransportMode _transportMode = ExampleTransportMode.mock;

  // ── Curve selection ───────────────────────────────────────────
  Curve _selectedCurve = Curve.secp256k1;

  // ── Stored state from keygen (used by sign/recovery/export) ──
  KeygenResult? _lastKeygen;
  String? _currentLocalShare; // updated after keygen or recovery
  String? _backupEnvelope; // created after keygen via deriveBackupEnvelope
  bool _exported = false;

  bool get _hasKey => _lastKeygen != null && _currentLocalShare != null;

  @override
  void initState() {
    super.initState();
    _rebuildClient(logChange: false);
  }

  @override
  void dispose() {
    _httpUrlController.dispose();
    _wsUrlController.dispose();
    unawaited(_disposeTransport(_transport));
    super.dispose();
  }

  void _log(String message) {
    debugPrint(message);
    setState(() => _logs.add(message));
  }

  void _clearLogs() {
    setState(() => _logs.clear());
  }

  Future<void> _switchTransport(ExampleTransportMode mode) async {
    if (_transportMode == mode) return;
    await _disposeTransport(_transport);
    setState(() {
      _transportMode = mode;
      _lastKeygen = null;
      _currentLocalShare = null;
      _backupEnvelope = null;
      _exported = false;
    });
    _rebuildClient();
  }

  void _rebuildClient({bool logChange = true}) {
    _transport = _createTransport();
    _engine = _useMockEngine ? MockMpcEngine() : MpcEngine(RustLib.instance.api);
    _client = MpcClient(engine: _engine, transport: _transport);
    if (logChange) {
      _log('Transport switched to ${_transportMode.label}.');
      _log(_transportMode.description);
      if (_transportMode == ExampleTransportMode.mock) {
        _log('Mock mode stays fully runnable without a backend.');
      } else {
        _log('Update the endpoint below and connect to a real MPC server.');
      }
    }
  }

  MpcTransport _createTransport() {
    return switch (_transportMode) {
      ExampleTransportMode.mock => MockMpcTransport(),
      ExampleTransportMode.http => HttpMpcTransport(rpcUrl: _httpUrlController.text.trim()),
      ExampleTransportMode.websocket => WebSocketMpcTransport(wsUrl: _wsUrlController.text.trim()),
    };
  }

  Future<void> _disposeTransport(MpcTransport transport) async {
    if (transport is WebSocketMpcTransport) {
      await transport.close();
    }
  }

  Widget _buildTransportCard() {
    final endpointController = switch (_transportMode) {
      ExampleTransportMode.http => _httpUrlController,
      ExampleTransportMode.websocket => _wsUrlController,
      ExampleTransportMode.mock => null,
    };

    return Card(
      margin: const EdgeInsets.all(8),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Transport Mode', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 12),
            SegmentedButton<ExampleTransportMode>(
              segments: ExampleTransportMode.values.map((mode) => ButtonSegment<ExampleTransportMode>(value: mode, label: Text(mode.label))).toList(),
              selected: {_transportMode},
              onSelectionChanged: (selection) {
                unawaited(_switchTransport(selection.first));
              },
            ),
            const SizedBox(height: 12),
            Text(_transportMode.description),
            if (endpointController != null) ...[
              const SizedBox(height: 12),
              TextField(
                controller: endpointController,
                decoration: InputDecoration(labelText: _transportMode == ExampleTransportMode.http ? 'HTTP RPC URL' : 'WebSocket URL', border: const OutlineInputBorder()),
                onSubmitted: (_) => _rebuildClient(),
              ),
              const SizedBox(height: 8),
              Text('Tip: switching transport only changes the injected constructor argument.', style: Theme.of(context).textTheme.bodySmall),
            ] else ...[
              const SizedBox(height: 8),
              Text('Mock mode uses in-memory server behavior so the example remains runnable offline.', style: Theme.of(context).textTheme.bodySmall),
            ],
            const SizedBox(height: 12),
            SelectableText(_transportSnippet(), style: Theme.of(context).textTheme.bodySmall?.copyWith(fontFamily: 'monospace')),
          ],
        ),
      ),
    );
  }

  /// Card for selecting the signing curve (EVM/secp256k1 or Solana/ed25519).
  Widget _buildCurveCard() {
    return Card(
      margin: const EdgeInsets.symmetric(horizontal: 8),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Curve / Chain', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 12),
            SegmentedButton<Curve>(
              segments: const [
                ButtonSegment<Curve>(
                  value: Curve.secp256k1,
                  label: Text('EVM'),
                ),
                ButtonSegment<Curve>(
                  value: Curve.ed25519,
                  label: Text('Solana'),
                ),
              ],
              selected: {_selectedCurve},
              onSelectionChanged: (selection) {
                setState(() {
                  _selectedCurve = selection.first;
                  // Clear key state when switching curves
                  _lastKeygen = null;
                  _currentLocalShare = null;
                  _backupEnvelope = null;
                  _exported = false;
                });
              },
            ),
            const SizedBox(height: 8),
            Text(
              _selectedCurve == Curve.secp256k1
                  ? 'DKLs23 ECDSA — 4-round keygen/sign/recovery. Returns EVM address (0x…).'
                  : 'FROST Ed25519 Schnorr — 3-round keygen/recovery, 2-round sign. Returns SOL address (base58).',
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ],
        ),
      ),
    );
  }

  String _transportSnippet() {
    return switch (_transportMode) {
      ExampleTransportMode.mock =>
        '''
final client = MpcClient(
  engine: MockMpcEngine(),
  transport: MockMpcTransport(),
);''',
      ExampleTransportMode.http =>
        '''
final client = MpcClient(
  engine: MpcEngine(RustLib.instance.api),
  transport: HttpMpcTransport(
    rpcUrl: '${_httpUrlController.text.trim()}',
  ),
);''',
      ExampleTransportMode.websocket =>
        '''
final client = MpcClient(
  engine: MpcEngine(RustLib.instance.api),
  transport: WebSocketMpcTransport(
    wsUrl: '${_wsUrlController.text.trim()}',
  ),
);''',
    };
  }

  // ── Example 1: Keygen ───────────────────────────────────────────

  /// Full keygen flow. After completion you get:
  /// - address: EVM address (secp256k1) or SOL address (ed25519)
  /// - publicKey: hex-encoded public key
  /// - localEncryptedShare: device key share (store in secure storage!)
  Future<void> _runKeygen() async {
    _clearLogs();
    final isSol = _selectedCurve == Curve.ed25519;
    _log('=== Keygen Example (${isSol ? "Solana / ed25519" : "EVM / secp256k1"}) ===');
    _log('Starting keygen...');

    try {
      final result = await _client.keygen(curve: _selectedCurve);

      setState(() {
        _lastKeygen = result;
        _currentLocalShare = result.localEncryptedShare;
        _exported = false;
      });

      _log('Keygen successful!');
      if (isSol) {
        _log('  SOL address: ${result.address}');
      } else {
        _log('  address: ${result.address}');
      }
      _log('  publicKey: ${result.publicKey.substring(0, 20)}...');
      _log('  mpcKeyId: ${result.mpcKeyId}');
      _log('  rotationVersion: ${result.rotationVersion}');
      _log('  localEncryptedShare: ${result.localEncryptedShare.length} chars');
      _log('');

      // Auto-create backup envelope (in real app, prompt user for secret)
      _log('Creating backup envelope...');
      final envelope = await _engine.deriveBackupEnvelope(result.localEncryptedShare, 'demo_backup_secret_123', DateTime.now().toUtc().toIso8601String());
      final envelopeJson = '{"version":"${envelope.version}","algorithm":"${envelope.algorithm}","created_at":"${envelope.createdAt}","payload":"${envelope.payload}"}';
      setState(() => _backupEnvelope = envelopeJson);
      _log('Backup envelope created (${envelopeJson.length} chars)');
      _log('');
      _log('Ready: Sign / Recovery / Export buttons are now active.');
    } on MpcProtocolException catch (e) {
      _log('Protocol error: ${e.message} (round: ${e.round})');
    } on MpcTransportException catch (e) {
      _log('Transport error: ${e.message} (method: ${e.method})');
    } catch (e) {
      _log('Unexpected error: $e');
    }
  }

  // ── Example 2: Recovery ─────────────────────────────────────────

  /// Recovery flow. Requires:
  /// - mpcKeyId: from the original keygen
  /// - encryptedBackupShare: the backup envelope stored by user
  /// - userBackupSecret: user's backup password/secret
  ///
  /// After recovery:
  /// - New localEncryptedShare (rotated, old one invalidated)
  /// - Same address as before
  /// - rotationVersion incremented
  Future<void> _runRecovery() async {
    _clearLogs();
    _log('=== Recovery Example ===');

    if (_lastKeygen == null || _backupEnvelope == null) {
      _log('Run keygen first (creates key + backup envelope).');
      return;
    }

    _log('Starting recovery for ${_lastKeygen!.mpcKeyId}...');
    _log('  Using backup envelope from keygen step');

    try {
      final result = await _client.recover(mpcKeyId: _lastKeygen!.mpcKeyId, encryptedBackupShare: _backupEnvelope!, userBackupSecret: 'demo_backup_secret_123', currentRotationVersion: _lastKeygen!.rotationVersion);

      // Update local share and rotation version (old share is now invalid).
      // Also re-derive the backup envelope so the next recovery uses the new share.
      setState(() {
        _currentLocalShare = result.localEncryptedShare;
        _lastKeygen = KeygenResult(
          mpcKeyId: _lastKeygen!.mpcKeyId,
          address: result.address,
          publicKey: result.publicKey,
          curve: _lastKeygen!.curve,
          threshold: _lastKeygen!.threshold,
          keyRef: _lastKeygen!.keyRef,
          backupState: _lastKeygen!.backupState,
          rotationVersion: result.rotationVersion,
          localEncryptedShare: result.localEncryptedShare,
        );
      });

      // Re-derive backup envelope from the new share so future recovery works.
      final newEnvelope = await _engine.deriveBackupEnvelope(
        result.localEncryptedShare,
        'demo_backup_secret_123',
        DateTime.now().toUtc().toIso8601String(),
      );
      setState(() => _backupEnvelope =
          '{"version":"${newEnvelope.version}","algorithm":"${newEnvelope.algorithm}","created_at":"${newEnvelope.createdAt}","payload":"${newEnvelope.payload}"}');

      _log('Recovery successful!');
      _log('  address: ${result.address}');
      _log('  rotationVersion: ${result.rotationVersion}');
      _log('  mpcKeyId: ${result.mpcKeyId}');
      _log('  localEncryptedShare updated (old one invalidated)');
      _log('  backupEnvelope re-derived from new share');
      _log('');
      _log('Address unchanged after recovery');
    } on MpcProtocolException catch (e) {
      _log('Protocol error: ${e.message}');
    } on MpcTransportException catch (e) {
      _log('Transport error: ${e.message}');
    } catch (e) {
      _log('Unexpected error: $e');
    }
  }

  // ── Example 3: Sign ─────────────────────────────────────────────

  /// Sign flow. Requires:
  /// - mpcKeyId: identifies which key pair to use
  /// - messageHash: keccak256 hash (EVM) or raw transaction bytes hex (SOL)
  /// - localEncryptedShare: device key share from keygen/recovery
  ///
  /// EVM: Returns (r, s, recid) to assemble an ECDSA signature.
  /// SOL: Returns 64-byte Schnorr signature via result.signatureHex (recid is null).
  Future<void> _runSign() async {
    _clearLogs();
    _log('=== Sign Example ===');

    if (_lastKeygen == null || _currentLocalShare == null) {
      _log('Run keygen first.');
      return;
    }
    if (_exported) {
      _log('Key already exported — signing disabled.');
      return;
    }

    final isSol = _selectedCurve == Curve.ed25519;
    final msgHash = 'aabbccdd' * 8; // 64-char hex (demo)
    _log('Signing...');
    _log('  messageHash: ${msgHash.substring(0, 16)}... (${isSol ? "raw tx bytes hex" : "keccak256"})');
    _log('  using localShare from ${_currentLocalShare == _lastKeygen!.localEncryptedShare ? "keygen" : "recovery"}');

    try {
      final result = await _client.sign(mpcKeyId: _lastKeygen!.mpcKeyId, messageHash: msgHash, localEncryptedShare: _currentLocalShare!);

      _log('Signing successful!');
      if (result.recid == null) {
        // ed25519 / FROST Schnorr
        _log('  signatureHex: ${result.signatureHex.substring(0, 20)}... (64-byte Schnorr, ed25519)');
        _log('  r: ${result.r.substring(0, 20)}...');
        _log('  s: ${result.s.substring(0, 20)}...');
        _log('');
        _log('Next steps:');
        _log('  1. Use signatureHex as the 64-byte Solana transaction signature');
        _log('  2. Attach to serialized Solana transaction and broadcast');
      } else {
        // secp256k1 / ECDSA
        _log('  r: ${result.r.substring(0, 20)}...');
        _log('  s: ${result.s.substring(0, 20)}...');
        _log('  recid: ${result.recid}');
        _log('');
        _log('Next steps:');
        _log('  1. Assemble signed transaction with (r, s, recid)');
        _log('  2. Broadcast to EVM chain');
      }
    } on MpcProtocolException catch (e) {
      _log('Protocol error: ${e.message}');
    } on MpcTransportException catch (e) {
      _log('Transport error: ${e.message}');
    } catch (e) {
      _log('Unexpected error: $e');
    }
  }

  // ── Example 4: Export Private Key ────────────────────────────────

  /// Export MPC wallet to a standard wallet.
  /// This reconstructs the full private key from both party shares.
  ///
  /// WARNING: After export, the MPC key pair should be considered compromised.
  /// The server marks the key as "exported" and disables further MPC operations.
  Future<void> _runExport() async {
    _clearLogs();
    _log('=== Export Private Key Example ===');

    if (_lastKeygen == null || _currentLocalShare == null) {
      _log('Run keygen first.');
      return;
    }
    if (_exported) {
      _log('Key already exported.');
      return;
    }

    _log('Requesting key export for ${_lastKeygen!.mpcKeyId}...');
    _log('WARNING: This will compromise the MPC key pair!');
    _log('');

    try {
      final result = await _client.exportPrivateKey(mpcKeyId: _lastKeygen!.mpcKeyId, localEncryptedShare: _currentLocalShare!);

      setState(() => _exported = true);

      _log('Export successful!');
      _log('  address: ${result.address}');
      _log('  privateKey: ${result.privateKey.substring(0, 10)}...[REDACTED]');
      _log('');
      _log('MPC key pair is now compromised.');
      _log('Sign / Recovery buttons disabled.');
    } on MpcProtocolException catch (e) {
      _log('Protocol error: ${e.message}');
    } on MpcTransportException catch (e) {
      _log('Transport error: ${e.message}');
    } catch (e) {
      _log('Unexpected error: $e');
    }
  }

  // ── Example 5: Error Handling ───────────────────────────────────

  /// Demonstrates how to handle different error types.
  Future<void> _runErrorDemo() async {
    _clearLogs();
    _log('=== Error Handling Example ===');
    _log('');

    // Example: catching specific exceptions
    _log('MpcProtocolException:');
    _log('  Thrown when Rust-side protocol returns error status.');
    _log('  Contains: message, round (which round failed).');
    _log('  Example: invalid server proof, verification failed.');
    _log('');
    _log('MpcTransportException:');
    _log('  Thrown when network communication fails.');
    _log('  Contains: message, endpoint, cause (original error).');
    _log('  Example: timeout, connection refused, 500 error.');
    _log('');

    _log('Pattern:');
    _log('  try {');
    _log('    await client.keygen();');
    _log('  } on MpcProtocolException catch (e) {');
    _log('    // Crypto error: show user, maybe retry');
    _log('    log("Protocol failed at round \${e.round}");');
    _log('  } on MpcTransportException catch (e) {');
    _log('    // Network error: retry with backoff');
    _log('    log("Network failed: \${e.method}");');
    _log('  }');
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('ceres_mpc Example')),
      body: Column(
        children: [
          // Controls area — scrollable so it never overflows on small screens.
          // Flexible (not Expanded) lets it shrink when the log area needs space.
          Flexible(
            child: SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _buildTransportCard(),
                  _buildCurveCard(),
                  // Action buttons
                  Padding(
                    padding: const EdgeInsets.all(8.0),
                    child: Wrap(
                      spacing: 8,
                      runSpacing: 8,
                      children: [
                        ElevatedButton(onPressed: _runKeygen, child: const Text('1. Keygen')),
                        ElevatedButton(onPressed: _hasKey && !_exported ? _runSign : null, child: const Text('2. Sign')),
                        ElevatedButton(onPressed: _hasKey && !_exported && _backupEnvelope != null ? _runRecovery : null, child: const Text('3. Recovery')),
                        ElevatedButton(
                          onPressed: _hasKey && !_exported ? _runExport : null,
                          style: ElevatedButton.styleFrom(backgroundColor: _hasKey && !_exported ? Colors.orange : null, foregroundColor: _hasKey && !_exported ? Colors.white : null),
                          child: const Text('4. Export'),
                        ),
                        OutlinedButton(onPressed: _runErrorDemo, child: const Text('Error Handling')),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
          const Divider(),
          // Log output
          Expanded(
            child: ListView.builder(
              padding: const EdgeInsets.all(8),
              itemCount: _logs.length,
              itemBuilder: (context, index) {
                return Text(_logs[index], style: const TextStyle(fontFamily: 'monospace', fontSize: 13));
              },
            ),
          ),
        ],
      ),
    );
  }
}
