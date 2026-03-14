//! SQLite persistence via sqlx.
//!
//! All database access goes through this module. Tables are created and
//! migrated via `db/migrations/`. Use `sqlx::query!` for compile-time
//! query checking.

mod queries;
mod sync;

pub use queries::Db;
pub use sync::sync_character;
