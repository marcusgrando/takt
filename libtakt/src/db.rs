use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::path::PathBuf;

pub async fn connect() -> anyhow::Result<SqlitePool> {
    let db_path = db_path()?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Opt-in DB reset: set TAKT_RESET_DB=1 to recreate DB on launch
    if std::env::var("TAKT_RESET_DB").is_ok() {
        let _ = std::fs::remove_file(&db_path);
    }

    let db_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("DB path contains invalid UTF-8"))?;
    let db_url = format!("sqlite://{}?mode=rwc", db_str);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&pool)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

fn db_path() -> anyhow::Result<PathBuf> {
    let base = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine data directory"))?
        .join("takt");
    if cfg!(debug_assertions) {
        Ok(base.join("takt-dev.db"))
    } else {
        Ok(base.join("takt.db"))
    }
}

#[cfg(test)]
pub async fn connect_in_memory() -> anyhow::Result<SqlitePool> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&pool)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn test_db_connects_and_migrates() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tasks")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);
    }
}
