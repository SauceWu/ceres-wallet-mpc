/// Transport abstraction injected by the host application.
///
/// The host fully controls HTTP headers, authentication, retries, and logging.
/// The SDK builds JSON-RPC 2.0 requests and passes the complete payload to [send].
///
/// Typically the host POSTs the payload to a single RPC endpoint (e.g. `/rpc`).
abstract class MpcTransport {
  /// Sends a JSON-RPC 2.0 [payload] to the server and returns the raw response.
  Future<String> send(String payload);
}
