//! Process-wide graceful-shutdown trigger shared by the server and tray.

use std::sync::OnceLock;

use tokio::sync::Notify;

fn notifier() -> &'static Notify {
    static NOTIFIER: OnceLock<Notify> = OnceLock::new();
    NOTIFIER.get_or_init(Notify::new)
}

/// Request that the server stop accepting new work and finish gracefully.
pub fn request() {
    notifier().notify_one();
}

/// Wait for an authenticated in-process shutdown request.
pub async fn wait() {
    notifier().notified().await;
}
