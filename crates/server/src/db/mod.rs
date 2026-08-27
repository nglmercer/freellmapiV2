//! Database modules, migrations, seeds, and key persistence.

pub mod connection;
pub mod migrations;
pub mod schema;
pub mod seed;
pub mod unified_key;

pub use connection::{db, db_path, init_db, run_in_transaction, set_db_path_override, DB};
pub use unified_key::{ensure_unified_key, get_unified_api_key, regenerate_unified_key};

pub use migrations::*;
pub use seed::*;

/// Seed the model catalog, apply data migrations in order, and ensure that a
/// unified API key exists. Every operation is idempotent.
pub fn run_migrations(conn: &rusqlite::Connection) {
    seed_models(conn);
    migrate_models(conn);
    migrate_models_v2(conn);
    migrate_models_v3_ranks(conn);
    migrate_models_v4(conn);
    migrate_models_v5(conn);
    migrate_models_v6(conn);
    migrate_models_v7(conn);
    migrate_models_v8(conn);
    migrate_models_v9(conn);
    migrate_models_v10(conn);
    migrate_models_v11(conn);
    migrate_models_v12(conn);
    migrate_models_v13(conn);
    migrate_models_v14(conn);
    ensure_unified_key(conn);
}
