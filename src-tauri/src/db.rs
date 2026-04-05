use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::path::PathBuf;

pub async fn connect() -> anyhow::Result<SqlitePool> {
    let db_path = db_path();
    std::fs::create_dir_all(db_path.parent().unwrap())?;

    // Dev mode: recreate DB every launch to avoid migration conflicts
    if cfg!(debug_assertions) {
        let _ = std::fs::remove_file(&db_path);
    }

    let db_url = format!("sqlite://{}?mode=rwc", db_path.to_str().unwrap());
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

fn db_path() -> PathBuf {
    let base = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cronmac");
    if cfg!(debug_assertions) {
        base.join("cronmac-dev.db")
    } else {
        base.join("cronmac.db")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
