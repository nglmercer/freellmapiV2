use server::db;
use server::env;

#[tokio::main]
async fn main() {
    env::init();
    server::logger::init();

    db::init_db(None).expect("failed to initialize database");
    db::run_in_transaction(|conn| {
        db::run_migrations(conn);
        Ok::<(), rusqlite::Error>(())
    })
    .await
    .expect("failed to run database migrations");

    server::services::state_persistence::restore_runtime_state();
    let app = server::app::create_app();

    let port = env::get_port();
    let bind_address = env::get_bind_address();
    let listener = tokio::net::TcpListener::bind((bind_address.as_str(), port))
        .await
        .expect("failed to bind port");

    println!("Server running on http://{bind_address}:{port}");
    println!("Proxy endpoint: http://{bind_address}:{port}/v1/chat/completions");

    server::services::health::start_health_checker();
    server::services::state_persistence::start_periodic_save(30_000);

    // Start model synchronization in the background without crashing the
    // server when a provider is unavailable.
    tokio::spawn(async {
        server::services::model_sync::run_initial_sync().await;
        server::services::model_sync::start_sync_scheduler();
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    println!("[StatePersistence] Shutting down, saving state...");
    if let Err(e) = server::services::state_persistence::save_runtime_state() {
        eprintln!("[StatePersistence] Shutdown save failed: {e}");
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                if let Err(error) = result {
                    eprintln!("failed to listen for SIGINT: {error}");
                }
            }
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("failed to listen for Ctrl-C: {error}");
    }
}
