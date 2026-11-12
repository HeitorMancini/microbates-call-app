use token_server::Config;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info")),
        )
        .init();

    let config = match Config::from_env() {
        Ok(config) => config,
        Err(e) => {
            tracing::error!("{e}");
            std::process::exit(2);
        }
    };

    let listener = match tokio::net::TcpListener::bind(config.bind).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!(bind = %config.bind, "failed to bind: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!(bind = %config.bind, livekit_url = %config.livekit_url, "token-server listening");

    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutting down");
    };
    if let Err(e) = token_server::serve(listener, config, shutdown).await {
        tracing::error!("server error: {e}");
        std::process::exit(1);
    }
}
