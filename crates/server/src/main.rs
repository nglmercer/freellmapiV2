use server::db;
use server::env;

#[tokio::main]
async fn main() {
    // Side effects of `import './env.js'`: ensure .env key, load .env, validate.
    env::init();
    server::logger::init();

    // Side effects of `import './db/index.js'`: initDb() + runMigrations().
    db::init_db(None).expect("failed to initialize database");
    {
        let conn = db::db().lock().await;
        db::run_migrations(&conn);
    }

    server::services::state_persistence::restore_runtime_state();
    let app = server::app::create_app();

    let port = env::get_port();
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("failed to bind port");

    println!("Server running on http://0.0.0.0:{port}");
    println!("Proxy endpoint: http://0.0.0.0:{port}/v1/chat/completions");

    server::services::health::start_health_checker();
    server::services::state_persistence::start_periodic_save(30_000);

    // start model sync in background without crashing server on failure
    tokio::spawn(async {
        server::services::model_sync::run_initial_sync().await;
        server::services::model_sync::start_sync_scheduler();
    });

    // Graceful shutdown on Ctrl-C — save runtime state first, like the TS
    // registerShutdownHandlers() does.
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
    tokio::signal::ctrl_c().await.ok();
}
