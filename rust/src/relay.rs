use futures_util::sink::Sink;
use futures_util::stream::Stream;
use sl_mpc_mate::coord::{MessageSendError, Relay};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

/// Bridge between FFI sync calls and sl-dkls23 async protocol.
/// Implements Stream + Sink + Unpin to satisfy the Relay trait.
///
/// - `rx`: receives messages from FFI side (server messages injected via continue)
/// - `tx`: sends messages to FFI side (client messages to return to caller)
pub struct ChannelRelayConn {
    pub rx: mpsc::Receiver<Vec<u8>>,
    pub tx: mpsc::UnboundedSender<Vec<u8>>,
}

impl Stream for ChannelRelayConn {
    type Item = Vec<u8>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Vec<u8>>> {
        self.rx.poll_recv(cx)
    }
}

impl Sink<Vec<u8>> for ChannelRelayConn {
    type Error = MessageSendError;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Vec<u8>) -> Result<(), Self::Error> {
        // UnboundedSender never blocks; ignore send error (task dropped = protocol done)
        self.get_mut().tx.send(item).map_err(|_| MessageSendError)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

// ChannelRelayConn is Unpin because both mpsc::Receiver and mpsc::UnboundedSender are Unpin.
impl Unpin for ChannelRelayConn {}

// Relay is a blanket trait alias: Stream<Item=Vec<u8>> + Sink<Vec<u8>, Error=MessageSendError> + Unpin
impl Relay for ChannelRelayConn {}
