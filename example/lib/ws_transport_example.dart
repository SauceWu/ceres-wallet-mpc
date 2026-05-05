library;

import 'dart:async';
import 'dart:convert';
import 'package:web_socket_channel/web_socket_channel.dart';
import 'package:ceres_mpc/ceres_mpc.dart';

/// WebSocket transport 实现 MpcTransport 接口。
///
/// 关键特性：
/// - 首次 send() 时自动建立 WebSocket 连接（懒连接）
/// - 断线后下次 send() 自动重连，调用方无感知
/// - 通过 JSON-RPC id 字段匹配并发请求和响应
/// - 连接或响应超时抛出 WsTransportTimeoutException
///
/// 用法：
/// ```dart
/// final transport = WebSocketMpcTransport(
///   wsUrl: 'ws://your-mpc-server.com/ws',
///   authToken: 'Bearer eyJhbG...',
/// );
/// final client = MpcClient(engine: ..., transport: transport);
/// // 使用完毕后释放资源
/// await transport.close();
/// ```
class WebSocketMpcTransport implements MpcTransport {
  final String wsUrl;
  final String? authToken;
  final Duration timeout;
  final Duration reconnectDelay;

  // 内部 channel factory，供测试注入 fake channel
  final WebSocketChannel Function(Uri)? _channelFactory;

  WebSocketChannel? _channel;
  StreamSubscription<dynamic>? _subscription;
  final Map<dynamic, Completer<String>> _pendingRequests = {};
  bool _connecting = false;
  Completer<void>? _connectCompleter;

  WebSocketMpcTransport({
    required this.wsUrl,
    this.authToken,
    this.timeout = const Duration(seconds: 30),
    this.reconnectDelay = const Duration(seconds: 1),
  }) : _channelFactory = null;

  /// 测试专用构造器，允许注入自定义 channel factory。
  // @visibleForTesting
  WebSocketMpcTransport.withChannelFactory({
    required this.wsUrl,
    required WebSocketChannel Function(Uri) channelFactory,
    this.authToken,
    this.timeout = const Duration(seconds: 30),
    this.reconnectDelay = const Duration(seconds: 1),
  }) : _channelFactory = channelFactory;

  @override
  Future<String> send(String payload) async {
    // 1. 确保连接就绪
    await _ensureConnected();

    // 2. 从 payload 提取 JSON-RPC id
    final Map<String, dynamic> decoded;
    try {
      decoded = jsonDecode(payload) as Map<String, dynamic>;
    } catch (_) {
      throw WsTransportException('Invalid JSON payload');
    }
    final id = decoded['id'];
    if (id == null) {
      throw WsTransportException('JSON-RPC payload must contain an id field');
    }

    // 3. 注册 Completer，发送请求
    final completer = Completer<String>();
    _pendingRequests[id] = completer;

    try {
      _channel!.sink.add(payload);
    } catch (e) {
      _pendingRequests.remove(id);
      // 连接已断，置空等待重连
      _channel = null;
      rethrow;
    }

    // 4. 等待响应，带超时
    try {
      return await completer.future.timeout(
        timeout,
        onTimeout: () {
          _pendingRequests.remove(id);
          throw WsTransportTimeoutException(
            'Request timed out after ${timeout.inSeconds}s (id=$id)',
          );
        },
      );
    } on WsTransportException {
      rethrow;
    }
  }

  Future<void> _ensureConnected() async {
    if (_channel != null) return;

    // 防止并发重复连接
    if (_connecting) {
      await _connectCompleter!.future;
      return;
    }

    _connecting = true;
    _connectCompleter = Completer<void>();

    try {
      // 使用注入的 factory（测试用）或真实 WebSocketChannel.connect
      _channel = (_channelFactory ?? WebSocketChannel.connect)(Uri.parse(wsUrl));

      // 等待握手完成（web_socket_channel 3.x 新增 ready）
      // 注意：authToken 存储于内存，不写入日志；
      // web_socket_channel 3.x 不支持 header 注入，
      // 生产环境建议通过 URL query 参数传递 token（由调用方控制）。
      await _channel!.ready.timeout(
        timeout,
        onTimeout: () {
          _channel = null;
          throw WsTransportTimeoutException(
            'WebSocket connection timed out after ${timeout.inSeconds}s',
          );
        },
      );

      // 注册消息监听
      _subscription = _channel!.stream.listen(
        _handleMessage,
        onError: _handleStreamError,
        onDone: _handleStreamDone,
        cancelOnError: false,
      );

      _connectCompleter!.complete();
    } catch (e) {
      _channel = null;
      _connectCompleter!.completeError(e);
      rethrow;
    } finally {
      _connecting = false;
      _connectCompleter = null;
    }
  }

  void _handleMessage(dynamic raw) {
    final String text;
    if (raw is String) {
      text = raw;
    } else if (raw is List<int>) {
      text = utf8.decode(raw);
    } else {
      return; // 忽略未知格式
    }

    Map<String, dynamic> response;
    try {
      response = jsonDecode(text) as Map<String, dynamic>;
    } catch (_) {
      return; // 非 JSON 消息静默忽略，不 crash（T-14-02）
    }

    final id = response['id'];
    if (id == null) return;

    // 仅接受与已注册 _pendingRequests key 匹配的 id，无匹配则静默丢弃（T-14-01）
    final completer = _pendingRequests.remove(id);
    if (completer == null) return;

    // Always complete with the raw JSON text — error parsing is the caller's job.
    // Throwing here would wrap JSON-RPC errors as transport exceptions and lose the
    // structured error code/message that MpcClient._rpcCall needs to throw MpcRpcException.
    completer.complete(text);
  }

  void _handleStreamError(Object error, StackTrace st) {
    _disconnect(WsTransportException('WebSocket stream error: $error'));
  }

  void _handleStreamDone() {
    _disconnect(WsTransportException('WebSocket connection closed'));
  }

  void _disconnect(WsTransportException cause) {
    _channel = null;
    _subscription?.cancel();
    _subscription = null;
    // 通知所有等待中的请求
    for (final completer in _pendingRequests.values) {
      if (!completer.isCompleted) completer.completeError(cause);
    }
    _pendingRequests.clear();
  }

  /// 主动关闭连接并释放资源。
  Future<void> close() async {
    final ch = _channel;
    _disconnect(WsTransportException('Transport closed'));
    await ch?.sink.close();
  }
}

/// 服务端返回 JSON-RPC error 对象，或 WebSocket 连接异常时抛出。
class WsTransportException implements Exception {
  final String message;
  final int? code;

  const WsTransportException(this.message, {this.code});

  @override
  String toString() =>
      code != null ? 'WsTransportException[$code]: $message' : 'WsTransportException: $message';
}

/// 连接建立或响应等待超时时抛出。
class WsTransportTimeoutException extends WsTransportException {
  const WsTransportTimeoutException(super.message) : super(code: null);

  @override
  String toString() => 'WsTransportTimeoutException: $message';
}
