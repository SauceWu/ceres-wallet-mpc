import 'dart:async';
import 'dart:convert';
import 'package:async/async.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:stream_channel/stream_channel.dart';
import 'package:web_socket_channel/web_socket_channel.dart';
import 'package:ceres_mpc_example/ws_transport_example.dart';

// ── Fake WebSocket helpers ────────────────────────────────────────

/// A fake [WebSocketChannel] backed by in-memory StreamControllers.
///
/// [pushResponse] simulates messages arriving FROM the server.
/// [disconnectServer] closes the inbound stream to simulate a disconnect.
class FakeWsSetup {
  /// Messages the transport sends TO the server.
  final StreamController<dynamic> _outbound = StreamController<dynamic>.broadcast();

  /// Messages the server sends TO the transport.
  final StreamController<dynamic> _inbound = StreamController<dynamic>.broadcast();

  late final WebSocketChannel channel;

  FakeWsSetup() {
    channel = _FakeWebSocketChannel(
      inboundStream: _inbound.stream,
      outboundSink: _outbound,
    );
  }

  /// Push a JSON-RPC response string to the transport's receive side.
  void pushResponse(String message) => _inbound.add(message);

  /// Close the inbound stream to simulate a server-side disconnect.
  Future<void> disconnectServer() async => _inbound.close();

  Stream<dynamic> get outboundMessages => _outbound.stream;
}

/// Fake [WebSocketChannel] that implements the interface class using
/// StreamChannelMixin.
class _FakeWebSocketChannel extends StreamChannelMixin<dynamic>
    implements WebSocketChannel {
  final Stream<dynamic> _stream;
  final StreamController<dynamic> _outboundCtrl;

  _FakeWebSocketChannel({
    required Stream<dynamic> inboundStream,
    required StreamController<dynamic> outboundSink,
  })  : _stream = inboundStream,
        _outboundCtrl = outboundSink;

  @override
  Stream<dynamic> get stream => _stream;

  @override
  late final WebSocketSink sink = _FakeSink(_outboundCtrl);

  @override
  Future<void> get ready => Future.value();

  @override
  int? get closeCode => null;

  @override
  String? get closeReason => null;

  @override
  String? get protocol => null;
}

class _FakeSink extends DelegatingStreamSink<dynamic> implements WebSocketSink {
  final StreamController<dynamic> _controller;

  _FakeSink(this._controller) : super(_controller.sink);

  @override
  Future<void> close([int? closeCode, String? closeReason]) async {
    await _controller.close();
  }
}

// ── Helper: build transport with fake channel factory ─────────────

WebSocketMpcTransport _makeTransport(
  FakeWsSetup setup, {
  Duration timeout = const Duration(seconds: 5),
}) {
  return WebSocketMpcTransport.withChannelFactory(
    wsUrl: 'ws://test-server',
    channelFactory: (_) => setup.channel,
    timeout: timeout,
  );
}

// ── Tests ─────────────────────────────────────────────────────────

void main() {
  group('WebSocketMpcTransport', () {
    test('T-1: send() 返回匹配 id 的响应', () async {
      final setup = FakeWsSetup();
      final transport = _makeTransport(setup);

      const payload = '{"jsonrpc":"2.0","method":"ping","id":1}';
      const response = '{"jsonrpc":"2.0","result":{"pong":true},"id":1}';

      // 发送后立即推送响应
      final future = transport.send(payload);
      await Future.delayed(Duration.zero); // 让连接完成
      setup.pushResponse(response);

      final result = await future;
      final decoded = jsonDecode(result) as Map<String, dynamic>;
      expect((decoded['result'] as Map)['pong'], isTrue);
    });

    test('T-2: 并发请求通过 id 独立匹配（乱序响应）', () async {
      final setup = FakeWsSetup();
      final transport = _makeTransport(setup);

      const p1 = '{"jsonrpc":"2.0","method":"a","id":1}';
      const p2 = '{"jsonrpc":"2.0","method":"b","id":2}';
      const r1 = '{"jsonrpc":"2.0","result":{"from":"id1"},"id":1}';
      const r2 = '{"jsonrpc":"2.0","result":{"from":"id2"},"id":2}';

      final f1 = transport.send(p1);
      final f2 = transport.send(p2);
      await Future.delayed(Duration.zero);

      // 乱序：先回 id=2，再回 id=1
      setup.pushResponse(r2);
      setup.pushResponse(r1);

      final results = await Future.wait([f1, f2]);
      final d1 = jsonDecode(results[0]) as Map<String, dynamic>;
      final d2 = jsonDecode(results[1]) as Map<String, dynamic>;
      expect((d1['result'] as Map)['from'], equals('id1'));
      expect((d2['result'] as Map)['from'], equals('id2'));
    });

    test('T-3: 服务端返回 error，send() 抛出 WsTransportException', () async {
      final setup = FakeWsSetup();
      final transport = _makeTransport(setup);

      const payload = '{"jsonrpc":"2.0","method":"x","id":3}';
      const errResponse =
          '{"jsonrpc":"2.0","error":{"code":-32001,"message":"Session expired"},"id":3}';

      final future = transport.send(payload);
      await Future.delayed(Duration.zero);
      setup.pushResponse(errResponse);

      expect(
        future,
        throwsA(
          isA<WsTransportException>()
              .having((e) => e.code, 'code', -32001)
              .having((e) => e.message, 'message', contains('Session expired')),
        ),
      );
    });

    test('T-4: 响应超时抛出 WsTransportTimeoutException', () async {
      final setup = FakeWsSetup();
      final transport = _makeTransport(
        setup,
        timeout: const Duration(milliseconds: 60),
      );

      const payload = '{"jsonrpc":"2.0","method":"slow","id":4}';
      // 不推送任何响应，让 timeout 触发

      await expectLater(
        transport.send(payload),
        throwsA(isA<WsTransportTimeoutException>()),
      );
    });

    test('T-5: stream 关闭后，下次 send() 自动重连', () async {
      var callCount = 0;
      final firstSetup = FakeWsSetup();
      final secondSetup = FakeWsSetup();

      final transport = WebSocketMpcTransport.withChannelFactory(
        wsUrl: 'ws://test-server',
        channelFactory: (_) {
          callCount++;
          return callCount == 1 ? firstSetup.channel : secondSetup.channel;
        },
      );

      // 第一次 send 成功
      const p1 = '{"jsonrpc":"2.0","method":"x","id":10}';
      const r1 = '{"jsonrpc":"2.0","result":{"ok":1},"id":10}';
      final f1 = transport.send(p1);
      await Future.delayed(Duration.zero);
      firstSetup.pushResponse(r1);
      await f1;

      // 模拟断线
      await firstSetup.disconnectServer();
      await Future.delayed(const Duration(milliseconds: 10));

      // 第二次 send 应自动使用新连接
      const p2 = '{"jsonrpc":"2.0","method":"y","id":11}';
      const r2 = '{"jsonrpc":"2.0","result":{"ok":2},"id":11}';
      final f2 = transport.send(p2);
      await Future.delayed(Duration.zero);
      secondSetup.pushResponse(r2);

      final result2 = await f2;
      final d2 = jsonDecode(result2) as Map<String, dynamic>;
      expect((d2['result'] as Map)['ok'], equals(2));
      expect(callCount, equals(2)); // 证明重连了
    });
  });
}
