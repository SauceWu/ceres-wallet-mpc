# ceres_mpc_example

Example Flutter app for the `ceres_mpc` SDK.

## What It Shows

- `MockMpcTransport` for a fully runnable local demo with no backend
- `HttpMpcTransport` as the JSON-RPC over HTTP reference implementation
- `WebSocketMpcTransport` as the persistent transport option for lower round-trip overhead

The app keeps transport injection explicit, so changing network mode is just a constructor swap.

If you want a backend to test against, use the server demo at [SauceWu/ceres-mpc-server-demo](https://github.com/SauceWu/ceres-mpc-server-demo).

## Packaging Note

Consumers should install `ceres_mpc` from `pub.dev`. They do not need to manually fetch Android AARs or iOS XCFrameworks.

For supported mobile targets, the native Rust library is expected to come from the package's signed precompiled release artifacts through `cargokit`.

## Running The Example

```bash
cd example
flutter pub get
flutter run
```

## Transport Modes

### Mock

Use this when you want to explore the UI and protocol flow without a live MPC server.

### HTTP

Update the RPC URL in the app, then wire `HttpMpcTransport` to your backend:

```dart
final client = MpcClient(
  engine: MpcEngine(RustLib.instance.api),
  transport: HttpMpcTransport(
    rpcUrl: 'https://your-mpc-server.com/rpc',
  ),
);
```

### WebSocket

Use the WebSocket transport when your server exposes a persistent JSON-RPC endpoint:

```dart
final client = MpcClient(
  engine: MpcEngine(RustLib.instance.api),
  transport: WebSocketMpcTransport(
    wsUrl: 'ws://your-mpc-server.com/ws',
    timeout: const Duration(seconds: 30),
  ),
);
```

`WebSocketMpcTransport` connects lazily on first `send()`, matches responses by JSON-RPC `id`, and reconnects on the next request after disconnect.
