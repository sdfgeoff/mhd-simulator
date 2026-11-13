pub mod caps;
pub mod domain;
pub mod gpu;
pub mod inputs;
pub mod model;
pub mod plane;
pub mod project;
pub mod simulation;
#[cfg(test)]
pub(crate) fn gpu_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}
