use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::sync::Arc;

pub type DbPool = Arc<Pool<SqliteConnectionManager>>;

pub fn create_pool(db_path: &str) -> DbPool {
    let manager = SqliteConnectionManager::file(db_path)
        .with_init(|conn| {
            conn.pragma_update(None, "foreign_keys", "ON")?;
            Ok(())
        });
    let pool = Pool::builder()
        .max_size(10)
        .build(manager)
        .expect("Failed to create connection pool");
    Arc::new(pool)
}

pub fn create_memory_pool() -> DbPool {
    let db_path = std::env::temp_dir().join(format!(
        "wisp-test-{}.db",
        uuid::Uuid::new_v4()
    ));
    let manager = SqliteConnectionManager::file(db_path)
        .with_init(|conn| {
            conn.pragma_update(None, "foreign_keys", "ON")?;
            Ok(())
        });
    let pool = Pool::builder()
        .max_size(10)
        .build(manager)
        .expect("Failed to create test connection pool");
    Arc::new(pool)
}
