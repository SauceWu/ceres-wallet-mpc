/// ceres_mpc SDK usage examples.
///
/// Demonstrates keygen, recovery, sign, backup, and error handling flows.
/// Uses [MockMpcTransport] to run without a real server.
///
/// For production usage with a real server, see [http_transport_example.dart].
// ignore_for_file: implementation_imports, invalid_use_of_internal_member
library;

import 'dart:async';

import 'package:flutter/material.dart';
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
  websocket(
    'WebSocket',
    'Persistent JSON-RPC over WebSocket with reconnect support.',
  );

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
  late MpcTransport _transport;
  final _httpUrlController = TextEditingController(
    text: 'http://127.0.0.1:3000/rpc',
  );
  final _wsUrlController = TextEditingController(
    text: 'ws://127.0.0.1:3000/ws',
  );
  ExampleTransportMode _transportMode = ExampleTransportMode.http;

  // Stored keygen result for recovery/sign demos
  KeygenResult? _lastKeygen;

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
    });
    _rebuildClient();
  }

  void _rebuildClient({bool logChange = true}) {
    _transport = _createTransport();
    _client = MpcClient(
      engine: _useMockEngine
          ? MockMpcEngine()
          : MpcEngine(RustLib.instance.api),
      transport: _transport,
    );
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
      ExampleTransportMode.http => HttpMpcTransport(
        rpcUrl: _httpUrlController.text.trim(),
      ),
      ExampleTransportMode.websocket => WebSocketMpcTransport(
        wsUrl: _wsUrlController.text.trim(),
      ),
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
            Text(
              'Transport Mode',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 12),
            SegmentedButton<ExampleTransportMode>(
              segments: ExampleTransportMode.values
                  .map(
                    (mode) => ButtonSegment<ExampleTransportMode>(
                      value: mode,
                      label: Text(mode.label),
                    ),
                  )
                  .toList(),
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
                decoration: InputDecoration(
                  labelText: _transportMode == ExampleTransportMode.http
                      ? 'HTTP RPC URL'
                      : 'WebSocket URL',
                  border: const OutlineInputBorder(),
                ),
                onSubmitted: (_) => _rebuildClient(),
              ),
              const SizedBox(height: 8),
              Text(
                'Tip: switching transport only changes the injected constructor argument.',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ] else ...[
              const SizedBox(height: 8),
              Text(
                'Mock mode uses in-memory server behavior so the example remains runnable offline.',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ],
            const SizedBox(height: 12),
            SelectableText(
              _transportSnippet(),
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                fontFamily: 'monospace',
              ),
            ),
          ],
        ),
      ),
    );
  }

  String _transportSnippet() {
    return switch (_transportMode) {
      ExampleTransportMode.mock => '''
final client = MpcClient(
  engine: MockMpcEngine(),
  transport: MockMpcTransport(),
);''',
      ExampleTransportMode.http => '''
final client = MpcClient(
  engine: MpcEngine(RustLib.instance.api),
  transport: HttpMpcTransport(
    rpcUrl: '${_httpUrlController.text.trim()}',
  ),
);''',
      ExampleTransportMode.websocket => '''
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
  /// - address: EVM address derived from group public key
  /// - publicKey: hex-encoded uncompressed secp256k1 public key
  /// - localEncryptedShare: device key share (store in secure storage!)
  Future<void> _runKeygen() async {
    _clearLogs();
    _log('=== Keygen Example ===');
    _log('Starting keygen...');

    try {
      final result = await _client.keygen();

      _lastKeygen = result;
      _log('Keygen successful!');
      _log('  address: ${result.address}');
      _log('  publicKey: ${result.publicKey.substring(0, 20)}...');
      _log('  curve: ${result.curve}');
      _log('  threshold: ${result.threshold}');
      _log('  rotationVersion: ${result.rotationVersion}');
      _log('  mpcKeyId: ${result.mpcKeyId}');
      _log('');
      _log('Next steps:');
      _log('  1. Store localEncryptedShare in device secure storage');
      _log('  2. Prompt user to create backup');
    } on MpcProtocolException catch (e) {
      _log('Protocol error: ${e.message} (round: ${e.round})');
    } on MpcTransportException catch (e) {
      _log('Transport error: ${e.message} (method: ${e.method})');
    } catch (e) {
      debugPrint('Unexpected error during keygen: $e');
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

    if (_lastKeygen == null) {
      _log('Run keygen first to get a key to recover.');
      return;
    }

    _log('Starting recovery for ${_lastKeygen!.mpcKeyId}...');

    try {
      final result = await _client.recover(
        mpcKeyId: _lastKeygen!.mpcKeyId,
        encryptedBackupShare: 'mock_encrypted_backup_data',
        userBackupSecret: 'user_secret_password_123',
        currentRotationVersion: _lastKeygen!.rotationVersion,
      );

      _log('Recovery successful!');
      _log('  address: ${result.address} (should match original)');
      _log('  rotationVersion: ${result.rotationVersion}');
      _log('  mpcKeyId: ${result.mpcKeyId}');
      _log('');
      _log('Key point: address is unchanged after recovery.');
      _log('Old key shares are now invalidated.');
    } on MpcProtocolException catch (e) {
      _log('Protocol error: ${e.message}');
    } on MpcTransportException catch (e) {
      _log('Transport error: ${e.message}');
    }
  }

  // ── Example 3: Sign ─────────────────────────────────────────────

  /// Sign flow. Requires:
  /// - mpcKeyId: identifies which key pair to use
  /// - messageHash: keccak256 hash of the unsigned transaction
  /// - localEncryptedShare: device key share from keygen/recovery
  ///
  /// Returns (r, s, recid) to assemble an ECDSA signature.
  Future<void> _runSign() async {
    _clearLogs();
    _log('=== Sign Example ===');

    if (_lastKeygen == null) {
      _log('Run keygen first.');
      return;
    }

    _log('Signing transaction...');
    _log('  messageHash: aabbccdd...');

    try {
      final result = await _client.sign(
        mpcKeyId: _lastKeygen!.mpcKeyId,
        messageHash: 'aabbccdd' * 8, // 64-char hex hash
        localEncryptedShare: _lastKeygen!.localEncryptedShare,
      );

      _log('Signing successful!');
      _log('  r: ${result.r.substring(0, 20)}...');
      _log('  s: ${result.s.substring(0, 20)}...');
      _log('  recid: ${result.recid}');
      _log('');
      _log('Next steps:');
      _log('  1. Assemble signed transaction with (r, s, recid)');
      _log('  2. Broadcast to EVM chain');
    } on MpcProtocolException catch (e) {
      _log('Protocol error: ${e.message}');
    } on MpcTransportException catch (e) {
      _log('Transport error: ${e.message}');
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

    if (_lastKeygen == null) {
      _log('Run keygen first.');
      return;
    }

    _log('Requesting key export for ${_lastKeygen!.mpcKeyId}...');
    _log('(Server requires strong authentication for this operation)');
    _log('');

    try {
      final result = await _client.exportPrivateKey(
        mpcKeyId: _lastKeygen!.mpcKeyId,
        localEncryptedShare: _lastKeygen!.localEncryptedShare,
      );

      _log('Export successful!');
      _log('  address: ${result.address}');
      _log('  privateKey: ${result.privateKey.substring(0, 10)}...[REDACTED]');
      _log('  exported: ${result.exported}');
      _log('');
      _log('You can now import this private key into MetaMask,');
      _log('Trust Wallet, or any standard EVM wallet.');
      _log('');
      _log('WARNING: This MPC key pair is now compromised.');
      _log('Do NOT continue using it for MPC operations.');
    } on MpcProtocolException catch (e) {
      _log('Protocol error: ${e.message}');
    } on MpcTransportException catch (e) {
      _log('Transport error: ${e.message}');
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
          _buildTransportCard(),
          // Action buttons
          Padding(
            padding: const EdgeInsets.all(8.0),
            child: Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                ElevatedButton(
                  onPressed: _runKeygen,
                  child: const Text('Keygen'),
                ),
                ElevatedButton(
                  onPressed: _runRecovery,
                  child: const Text('Recovery'),
                ),
                ElevatedButton(
                  onPressed: _runSign,
                  child: const Text('Sign'),
                ),
                ElevatedButton(
                  onPressed: _runExport,
                  style: ElevatedButton.styleFrom(
                    backgroundColor: Colors.orange,
                    foregroundColor: Colors.white,
                  ),
                  child: const Text('Export Key'),
                ),
                OutlinedButton(
                  onPressed: _runErrorDemo,
                  child: const Text('Error Handling'),
                ),
              ],
            ),
          ),
          const Divider(),
          // Log output
          Expanded(
            child: ListView.builder(
              padding: const EdgeInsets.all(8),
              itemCount: _logs.length,
              itemBuilder: (context, index) {
                return Text(
                  _logs[index],
                  style: const TextStyle(
                    fontFamily: 'monospace',
                    fontSize: 13,
                  ),
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}
