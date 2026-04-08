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
  final String endpoint;
  final Object? cause;

  const MpcTransportException(this.message,
      {required this.endpoint, this.cause});

  @override
  String toString() =>
      'MpcTransportException(endpoint: $endpoint, message: $message)';
}
