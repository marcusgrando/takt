use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::sync::oneshot;

/// Trait implemented by the host platform (Swift on macOS, C# on Windows).
/// Uses UniFFI `with_foreign` — gives `Arc<dyn PlatformBridge>` on Rust side.
#[uniffi::export(with_foreign)]
pub trait PlatformBridge: Send + Sync {
    /// Send a native notification.
    fn send_notification(&self, title: String, body: String, sound: bool);

    /// Execute a registered callback on the main thread.
    /// Swift MUST use DispatchQueue.main.async (not .sync) to avoid deadlock,
    /// then call `execute_callback(callbackId)` to run the closure.
    fn run_on_main_sync(&self, callback_id: u64);
}

// ── Callback Registry ────────────────────────────────────────────────

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

type BoxedCallback = Box<dyn FnOnce() + Send + 'static>;

static REGISTRY: Mutex<Option<HashMap<u64, BoxedCallback>>> = Mutex::new(None);

fn registry() -> std::sync::MutexGuard<'static, Option<HashMap<u64, BoxedCallback>>> {
    let mut guard = REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard
}

/// Register a callback and return its ID.
fn register_callback(f: BoxedCallback) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    registry().as_mut().unwrap().insert(id, f);
    id
}

/// Execute and remove a callback by ID. Called from Swift via FFI.
#[uniffi::export]
pub fn execute_callback(callback_id: u64) {
    let cb = registry().as_mut().and_then(|m| m.remove(&callback_id));
    if let Some(f) = cb {
        f();
    }
}

/// Run a closure on the main thread via PlatformBridge and wait for completion.
/// Uses oneshot channel so the calling tokio task yields instead of blocking.
pub async fn run_on_main<F, R>(bridge: &dyn PlatformBridge, f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    let callback = Box::new(move || {
        let result = f();
        let _ = tx.send(result);
    });
    let id = register_callback(callback);
    bridge.run_on_main_sync(id);
    rx.await
        .expect("Main thread callback was dropped without executing")
}
