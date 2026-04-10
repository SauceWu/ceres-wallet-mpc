/// Thrown when the MPC protocol returns an error status from the Rust layer.
class MpcProtocolException implements Exception {
  final String message;
  final int? round;

  const MpcProtocolException(this.message, {this.round});

  @override
  String toString() => 'MpcProtocolException(message: $message, round: $round)';
}

/// Thrown when the transport layer (network) fails during MPC round-trip.
class MpcTransportException implements Exception {
  final String message;
  final String? method;
  final Object? cause;

  const MpcTransportException(this.message, {this.method, this.cause});

  @override
  String toString() =>
      'MpcTransportException(method: $method, message: $message)';
}

/// Thrown when the server returns a JSON-RPC 2.0 error object.
///
/// Standard JSON-RPC error codes:
/// - `-32700` Parse error
/// - `-32600` Invalid request
/// - `-32601` Method not found
///
/// Application-defined error codes:
/// - `-32001` Session not found / expired
/// - `-32002` Verification failed
/// - `-32003` Key not found
/// - `-32004` Key already exported
class MpcRpcException implements Exception {
  final int code;
  final String message;
  final Object? data;

  const MpcRpcException({
    required this.code,
    required this.message,
    this.data,
  });

  /// Standard JSON-RPC error codes.
  static const parseError = -32700;
  static const invalidRequest = -32600;
  static const methodNotFound = -32601;

  /// Application-defined error codes.
  static const sessionNotFound = -32001;
  static const verificationFailed = -32002;
  static const keyNotFound = -32003;
  static const keyAlreadyExported = -32004;

  @override
  String toString() =>
      'MpcRpcException(code: $code, message: $message, data: $data)';
}
