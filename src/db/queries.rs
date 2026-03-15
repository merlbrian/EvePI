use anyhow::Context as _;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

/// Manages the SQLite connection pool.
#[derive(Debug, Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// Open (or create) the SQLite database.
    ///
    /// The path is resolved in order:
    /// 1. `EVEPI_DB_PATH` environment variable (allows tests to inject `:memory:` or a temp file).
    /// 2. Default: `~/.config/evepi/evepi.db`.
    pub async fn open() -> anyhow::Result<Self> {
        if let Ok(path) = std::env::var("EVEPI_DB_PATH") {
            return Self::open_at(&path).await;
        }
        let db_path = dirs::config_dir()
            .context("cannot determine config dir")?
            .join("evepi")
            .join("evepi.db");
        std::fs::create_dir_all(db_path.parent().unwrap())
            .context("failed to create evepi config dir")?;
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        Self::open_url(&url).await
    }

    /// Open (or create) the SQLite database at an explicit path.
    ///
    /// Pass `":memory:"` for an in-memory database that is useful in tests.
    pub async fn open_at(path: &str) -> anyhow::Result<Self> {
        let url = if path == ":memory:" {
            "sqlite::memory:".to_owned()
        } else {
            format!("sqlite://{}?mode=rwc", path)
        };
        Self::open_url(&url).await
    }

    async fn open_url(url: &str) -> anyhow::Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(url)
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
