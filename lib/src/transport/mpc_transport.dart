/// Transport abstraction injected by the host application (per D-02, D-05, D-07).
///
/// The host fully controls HTTP headers, authentication, retries, and logging.
/// The SDK only calls [send] during MPC round-trip orchestration.
abstract class MpcTransport {
  /// Sends [payload] to the given [endpoint] and returns the server response.
  Future<String> send(String endpoint, String payload);
}
