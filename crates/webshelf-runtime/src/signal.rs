/// Wait for shutdown signal (SIGTERM or SIGINT).
///
/// 跨平台实现：Unix 系统监听 SIGTERM + Ctrl+C；非 Unix 系统仅监听 Ctrl+C。
/// 提取到 `webshelf-runtime` 中，所有框架适配器复用，避免代码重复。
pub async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
        tracing::info!("Received Ctrl+C signal, initiating graceful shutdown");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
        tracing::info!("Received SIGTERM signal, initiating graceful shutdown");
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
