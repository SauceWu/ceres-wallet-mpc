//! 批量消息收集机制的集成测试
//! 验证 ChannelRelayConn Notify + collect_batch 的端到端行为

#[cfg(test)]
mod tests {
    use ceres_mpc::api::types::{ProtocolType, WireEnvelope};
    use ceres_mpc::relay::ChannelRelayConn;
    use futures_util::StreamExt;
    use std::sync::Arc;
    use tokio::sync::{mpsc, Notify};

    /// 测试 ChannelRelayConn::new 返回 (Self, Arc<Notify>)，且 notify 引用计数 >= 2
    #[test]
    fn test_channel_relay_conn_new() {
        let (tx_in, rx_in) = mpsc::channel::<Vec<u8>>(32);
        let (tx_out, _rx_out) = mpsc::channel::<Vec<u8>>(64);
        let (conn, notify) = ChannelRelayConn::new(rx_in, tx_out);
        // conn 内部持有一个 clone，外部持有另一个 → strong_count >= 2
        assert!(Arc::strong_count(&notify) >= 2);
        drop(conn);
        drop(tx_in);
    }

    /// 测试：模拟协议 task 发送 3 条消息后进入等待，collect_batch 能收集全部。
    /// 这是 BATCH-04 的核心验证：端到端批量收集。
    ///
    /// 时序安全验证（T-16-09）：collect_batch 先 subscribe notified() 再 recv()，
    /// 因此即使协议在 recv 返回后立刻 notify，通知也不会丢失。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_batch_collect_simulated_round() {
        let (tx_in, rx_in) = mpsc::channel::<Vec<u8>>(32);
        let (tx_out, rx_out) = mpsc::channel::<Vec<u8>>(64);
        let (conn, round_complete) = ChannelRelayConn::new(rx_in, tx_out);

        // spawn 模拟协议 task：
        // 1. 通过 conn (Sink) 发送 3 条消息到 tx_out（即 rx_out 端）
        // 2. 然后调用 conn.next()（Stream）等待输入 → 触发 round_complete Notify
        let task = tokio::spawn(async move {
            use futures_util::SinkExt;
            let mut conn = conn;
            conn.send(vec![1u8]).await.ok();
            conn.send(vec![2u8]).await.ok();
            conn.send(vec![3u8]).await.ok();
            // 等待输入（触发 poll_next → Pending → notify_one）
            conn.next().await;
        });

        // 在同步上下文中调用 collect_batch
        let result = tokio::task::spawn_blocking(move || {
            use ceres_mpc::runtime::get_runtime;
            let mut rx_out = rx_out;

            // collect_batch 逻辑（直接内联，因为 collect_batch 是私有函数）
            // Step 1: 先 subscribe
            let notified = round_complete.notified();
            // Step 2: 等第一条消息
            let first = get_runtime().block_on(rx_out.recv())?;
            let mut messages = vec![first];
            // Step 3: 等 Notify
            get_runtime().block_on(notified);
            // Step 4: drain 剩余
            while let Ok(msg) = rx_out.try_recv() {
                messages.push(msg);
            }
            Some(messages)
        })
        .await
        .unwrap();

        assert!(result.is_some());
        let msgs = result.unwrap();
        assert_eq!(msgs.len(), 3, "应收集到 3 条消息，实际收到 {}", msgs.len());
        assert_eq!(msgs[0], vec![1u8]);
        assert_eq!(msgs[1], vec![2u8]);
        assert_eq!(msgs[2], vec![3u8]);

        // 让协议 task 完成
        drop(tx_in);
        task.await.ok();
    }

    /// 测试：协议 task 完成（conn drop）时，rx_out.recv() 返回 None → collect_batch 返回 None
    /// T-16-02: 收集方在 rx 关闭时不死锁，正确返回 None。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_batch_collect_protocol_complete() {
        let (_tx_in, rx_in) = mpsc::channel::<Vec<u8>>(32);
        let (tx_out, rx_out) = mpsc::channel::<Vec<u8>>(64);
        let (conn, round_complete) = ChannelRelayConn::new(rx_in, tx_out);

        // drop conn → conn 内的 tx (Sender 的唯一 clone) 被 drop → rx_out 关闭
        drop(conn);

        let result = tokio::task::spawn_blocking(move || {
            use ceres_mpc::runtime::get_runtime;
            let mut rx_out = rx_out;
            // Step 1: subscribe
            let notified = round_complete.notified();
            // Step 2: recv() → None（channel 已关闭）
            let first = get_runtime().block_on(rx_out.recv());
            drop(notified); // channel 关闭，不等 notify
            first.map(|f| {
                let mut messages = vec![f];
                while let Ok(msg) = rx_out.try_recv() {
                    messages.push(msg);
                }
                messages
            })
        })
        .await
        .unwrap();

        assert!(result.is_none(), "channel 关闭时应返回 None");
    }

    /// 测试：WireEnvelope batch payloads 编解码往返（new_batch + decode_all_payloads）
    #[test]
    fn test_wire_envelope_batch_roundtrip() {
        let payloads = vec!["AQID".to_string(), "BAUG".to_string()]; // [1,2,3], [4,5,6]
        let env = WireEnvelope::new_batch(
            "aabbccdd".to_string(),
            ProtocolType::Dkg,
            1,
            0,
            Some(1),
            payloads,
            None,
        );

        // 序列化包含 payloads 字段
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("payloads"), "JSON 应包含 payloads 字段");

        // 反序列化后 decode_all_payloads 正确
        let restored: WireEnvelope = serde_json::from_str(&json).unwrap();
        let decoded = restored.decode_all_payloads().unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0], vec![1u8, 2, 3]);
        assert_eq!(decoded[1], vec![4u8, 5, 6]);
    }

    /// 测试：旧 WireEnvelope（无 payloads 字段）的 decode_all_payloads 返回单条消息，向后兼容
    #[test]
    fn test_wire_envelope_single_payload_compat() {
        let env = WireEnvelope::new(
            "aabbccdd".to_string(),
            ProtocolType::Dsg,
            2,
            1,
            Some(0),
            "AQID".to_string(), // base64([1,2,3])
            None,
        );

        assert!(env.payloads.is_none(), "旧格式不应有 payloads 字段");
        let decoded = env.decode_all_payloads().unwrap();
        assert_eq!(decoded.len(), 1, "旧格式应返回 1 条消息");
        assert_eq!(decoded[0], vec![1u8, 2, 3]);
    }

    /// 测试：Notify 幂等性 — 多次 notify_one 在 notified 消费前等效一次
    #[tokio::test]
    async fn test_notify_idempotent() {
        let notify = Arc::new(Notify::new());
        notify.notify_one();
        notify.notify_one(); // 第二次应幂等
        notify.notified().await; // 应立即返回，不 hang
    }
}
