use once_cell::sync::OnceCell;
use tokio::runtime::Runtime;

static RUNTIME: OnceCell<Runtime> = OnceCell::new();

/// Get or initialize the process-level tokio current_thread runtime.
/// Used at every FFI -> async boundary (keygen_start, sign_start, etc.).
/// Never destroyed during process lifetime.
pub fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime init failed")
    })
}
