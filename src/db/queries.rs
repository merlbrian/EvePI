use anyhow::Context as _;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

/// Manages the SQLite connection pool.
#[derive(Debug, Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// Open (or create) the SQLite database at the default path
    /// `~/.config/evepi/evepi.db` and run all pending migrations.
    pub async fn open() -> anyhow::Result<Self> {
        let db_path = dirs::config_dir()
            .context("cannot determine config dir")?
            .join("evepi")
            .join("evepi.db");
        std::fs::create_dir_all(db_path.parent().unwrap())
            .context("failed to create evepi config dir")?;
        let url = format!("sqlite://{}?mode=rwc", db_path.display());

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .context("failed to open SQLite database")?;

        sqlx::migrate!("./db/migrations")
            .run(&pool)
            .await
            .context("database migration failed")?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
