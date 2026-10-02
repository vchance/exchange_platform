//! When a process is asked to stop: Ctrl-C at a terminal, or the `TERM`
//! signal a container runtime or service manager sends. Both mean the same
//! thing, and a process that heeds only one is killed by the other.

/// Resolves when the process has been asked to stop.
pub async fn signal() {
    let interrupt = async {
        tokio::signal::ctrl_c()
            .await
            .expect("the interrupt signal can be listened for");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("the terminate signal can be listened for")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = interrupt => {}
        _ = terminate => {}
    }
}
