use once_cell::sync::OnceCell;
use tokio::runtime::Runtime;

static RUNTIME: OnceCell<Runtime> = OnceCell::new();

/// Get or initialize the process-level tokio multi-thread runtime.
/// sl-dkls23 internally calls spawn_blocking (dkg.rs:228), which
/// requires a multi-thread runtime to avoid deadlocks.
pub fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime init failed")
    })
}
