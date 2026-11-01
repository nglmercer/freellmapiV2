//! A database created before usernames existed must start, gain the `username`
//! column, and give its existing owner that username. Regression test for
//! `no such column: username` on a database created by an earlier version.

use better_auth::core::{migration::MigrationExecutor, options::PasswordHashOptions};
use server::auth::sqlite::SqliteDbAdapter;

fn temp_db_path() -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("legacy-auth-{}-{nanos}.db", std::process::id()))
}

#[tokio::test]
async fn startup_adds_username_and_backfills_the_legacy_owner() {
    let path = temp_db_path();
    {
        // The `user` table and owner as the email-only version stored them.
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"CREATE TABLE "user" ("additional_fields" TEXT, "email" TEXT NOT NULL,
               "email_verified" BOOLEAN NOT NULL, "id" TEXT NOT NULL PRIMARY KEY,
               "image" TEXT, "name" TEXT NOT NULL);
               INSERT INTO "user" (id, email, email_verified, name)
               VALUES ('legacy-owner', 'memelser@accounts.freellmapi.local', 0, 'memelser');"#,
        )
        .unwrap();
    }

    let adapter = SqliteDbAdapter::open(&path).expect("open legacy database");
    adapter
        .execute("SELECT 1")
        .await
        .expect("adapter usable before startup");
    server::auth::init(
        adapter,
        "x".repeat(40),
        PasswordHashOptions {
            scrypt_log_n: 10,
            ..PasswordHashOptions::default()
        },
    )
    .await
    .expect("startup must upgrade the legacy user table");

    let conn = rusqlite::Connection::open(&path).unwrap();
    let username: Option<String> = conn
        .query_row(
            "SELECT username FROM \"user\" WHERE id = 'legacy-owner'",
            [],
            |row| row.get(0),
        )
        .expect("username column exists");
    assert_eq!(username.as_deref(), Some("memelser"));
    drop(conn);
    std::fs::remove_file(&path).ok();
}
