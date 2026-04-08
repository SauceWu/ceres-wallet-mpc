/// Example: Real HTTP transport implementation for production use.
///
/// With JSON-RPC 2.0, all requests go to a single endpoint (e.g. `/rpc`).
/// The server routes based on the `method` field in the request body.
///
/// Dependencies needed in your app's pubspec.yaml:
///   dependencies:
///     http: ^1.0.0
///     ceres_mpc:
///       git:
///         url: https://github.com/SauceWu/ceres-mpc.git
library;

import 'package:ceres_mpc/ceres_mpc.dart';

// ignore: depend_on_referenced_packages
// In real usage, add `http` to your pubspec.yaml dependencies.
// import 'package:http/http.dart' as http;

/// Production HTTP transport for JSON-RPC 2.0.
///
/// All MPC requests are POSTed to a single URL. The server dispatches
/// based on the `method` field in the JSON-RPC request body.
///
/// Usage:
/// ```dart
/// final transport = HttpMpcTransport(
///   rpcUrl: 'https://your-mpc-server.com/rpc',
///   authToken: 'Bearer eyJhbG...',
/// );
///
/// final client = MpcClient(
///   engine: MpcEngine(RustLib.instance.api),
///   transport: transport,
/// );
/// ```
class HttpMpcTransport implements MpcTransport {
  /// Single JSON-RPC endpoint URL (e.g. `https://api.example.com/rpc`).
  final String rpcUrl;
  final String? authToken;
  final Duration timeout;

  HttpMpcTransport({
    required this.rpcUrl,
    this.authToken,
    this.timeout = const Duration(seconds: 30),
  });

  @override
  Future<String> send(String payload) async {
    // ---------------------------------------------------------------
    // Uncomment below when using in a real project with `http` package:
    // ---------------------------------------------------------------
    //
    // final url = Uri.parse(rpcUrl);
    //
    // final headers = <String, String>{
    //   'Content-Type': 'application/json',
    //   if (authToken != null) 'Authorization': authToken!,
    // };
    //
    // final response = await http.post(
    //   url,
    //   headers: headers,
    //   body: payload,
    // ).timeout(timeout);
    //
    // if (response.statusCode != 200) {
    //   throw Exception(
    //     'MPC server error: ${response.statusCode} ${response.body}',
    //   );
    // }
    //
    // return response.body;

    // Placeholder — replace with the above when http package is available.
    throw UnimplementedError(
      'Add http package to pubspec.yaml and uncomment the real implementation',
    );
  }
}

/// Example: Transport with retry logic for unreliable networks.
class RetryHttpMpcTransport implements MpcTransport {
  final HttpMpcTransport _inner;
  final int maxRetries;
  final Duration retryDelay;

  RetryHttpMpcTransport({
    required HttpMpcTransport inner,
    this.maxRetries = 3,
    this.retryDelay = const Duration(seconds: 1),
  }) : _inner = inner;

  @override
  Future<String> send(String payload) async {
    Exception? lastError;

    for (var attempt = 0; attempt <= maxRetries; attempt++) {
      try {
        return await _inner.send(payload);
      } on Exception catch (e) {
        lastError = e;
        if (attempt < maxRetries) {
          await Future.delayed(retryDelay * (attempt + 1));
        }
      }
    }

    throw lastError!;
  }
}

/// Example: Transport with request/response logging for debugging.
class LoggingMpcTransport implements MpcTransport {
  final MpcTransport _inner;
  final void Function(String message) _log;

  LoggingMpcTransport({
    required MpcTransport inner,
    void Function(String message)? log,
  })  : _inner = inner,
        _log = log ?? print;

  @override
  Future<String> send(String payload) async {
    _log('[MPC-RPC] --> ${_truncate(payload, 200)}');

    final stopwatch = Stopwatch()..start();
    try {
      final response = await _inner.send(payload);
      stopwatch.stop();
      _log('[MPC-RPC] <-- (${stopwatch.elapsedMilliseconds}ms) ${_truncate(response, 200)}');
      return response;
    } catch (e) {
      stopwatch.stop();
      _log('[MPC-RPC] <-- FAILED (${stopwatch.elapsedMilliseconds}ms): $e');
      rethrow;
    }
  }

  String _truncate(String s, int maxLen) {
    return s.length <= maxLen ? s : '${s.substring(0, maxLen)}...';
  }
}
