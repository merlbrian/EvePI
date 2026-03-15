//! SQLite persistence via sqlx.
//!
//! All database access goes through this module. Tables are created and
//! migrated via `db/migrations/`. Use `sqlx::query!` for compile-time
//! query checking.

mod queries;
mod sync;

pub use queries::Db;
pub use sync::sync_character;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::Db;

    /// Open a fresh in-memory database with all migrations applied.
    async fn test_db() -> Db {
        Db::open_at(":memory:")
            .await
            .expect("failed to open in-memory test database")
    }

    #[tokio::test]
    async fn migrations_apply_cleanly() {
        // If this completes without error, all migrations ran cleanly.
        test_db().await;
    }

    #[tokio::test]
    async fn upsert_account_idempotent() {
        let db = test_db().await;

        for _ in 0..2 {
            sqlx::query("INSERT OR IGNORE INTO accounts (label) VALUES (?1)")
                .bind("test-account")
                .execute(db.pool())
                .await
                .expect("insert account");
        }

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE label = ?1")
            .bind("test-account")
            .fetch_one(db.pool())
            .await
            .expect("count accounts");

        assert_eq!(count, 1, "idempotent insert should leave exactly one row");
    }

    #[tokio::test]
    async fn assign_planet_indices_gas_iii() {
        let db = test_db().await;

        // Seed an account and character.
        sqlx::query("INSERT INTO accounts (label) VALUES ('acct')")
            .execute(db.pool())
            .await
            .expect("insert account");
        let account_id: i64 =
            sqlx::query_scalar("SELECT account_id FROM accounts WHERE label = 'acct'")
                .fetch_one(db.pool())
                .await
                .expect("fetch account_id");

        sqlx::query(
            "INSERT INTO characters (character_id, account_id, character_name)
             VALUES (1, ?1, 'Tester')",
        )
        .bind(account_id)
        .execute(db.pool())
        .await
        .expect("insert character");

        // Seed three Gas planets in the same system with ascending planet_ids.
        for planet_id in [100i64, 200, 300] {
            sqlx::query(
                "INSERT INTO colonies
                 (character_id, planet_id, planet_type, solar_system_id)
                 VALUES (1, ?1, 'Gas', 999)",
            )
            .bind(planet_id)
            .execute(db.pool())
            .await
            .expect("insert colony");
        }

        // Apply the same window-function ranking used by sync::assign_planet_indices.
        sqlx::query(
            "UPDATE colonies
             SET planet_index = sub.rn
             FROM (
                 SELECT planet_id,
                        ROW_NUMBER() OVER (
                            PARTITION BY character_id, solar_system_id, planet_type
                            ORDER BY planet_id
                        ) AS rn
                 FROM colonies
                 WHERE character_id = 1
             ) AS sub
             WHERE colonies.planet_id = sub.planet_id
               AND colonies.character_id = 1",
        )
        .execute(db.pool())
        .await
        .expect("assign planet indices");

        let indices: Vec<i64> = sqlx::query_scalar(
            "SELECT planet_index FROM colonies WHERE character_id = 1 ORDER BY planet_id",
        )
        .fetch_all(db.pool())
        .await
        .expect("fetch indices");

        assert_eq!(
            indices,
            vec![1, 2, 3],
            "three Gas planets should receive indices 1, 2, 3"
        );
    }

    #[tokio::test]
    async fn list_colonies_filter_by_character() {
        let db = test_db().await;

        // Two accounts and two characters.
        for (label, char_id, char_name) in [("acct-a", 1i64, "Alice"), ("acct-b", 2i64, "Bob")] {
            sqlx::query("INSERT INTO accounts (label) VALUES (?1)")
                .bind(label)
                .execute(db.pool())
                .await
                .expect("insert account");
            let account_id: i64 =
                sqlx::query_scalar("SELECT account_id FROM accounts WHERE label = ?1")
                    .bind(label)
                    .fetch_one(db.pool())
                    .await
                    .expect("fetch account_id");
            sqlx::query(
                "INSERT INTO characters (character_id, account_id, character_name)
                 VALUES (?1, ?2, ?3)",
            )
            .bind(char_id)
            .bind(account_id)
            .bind(char_name)
            .execute(db.pool())
            .await
            .expect("insert character");
        }

        // Alice has 3 colonies, Bob has 1.
        for planet_id in [10i64, 20, 30] {
            sqlx::query(
                "INSERT INTO colonies
                 (character_id, planet_id, planet_type, solar_system_id)
                 VALUES (1, ?1, 'Gas', 999)",
            )
            .bind(planet_id)
            .execute(db.pool())
            .await
            .expect("insert alice colony");
        }
        sqlx::query(
            "INSERT INTO colonies
             (character_id, planet_id, planet_type, solar_system_id)
             VALUES (2, 40, 'Lava', 999)",
        )
        .execute(db.pool())
        .await
        .expect("insert bob colony");

        let alice_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM colonies WHERE character_id = 1")
                .fetch_one(db.pool())
                .await
                .expect("count alice colonies");
        assert_eq!(alice_count, 3);

        let bob_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM colonies WHERE character_id = 2")
                .fetch_one(db.pool())
                .await
                .expect("count bob colonies");
        assert_eq!(bob_count, 1);
    }

    #[tokio::test]
    async fn customs_office_upsert_and_update() {
        let db = test_db().await;

        sqlx::query(
            "INSERT OR REPLACE INTO customs_offices (planet_id, tax_rate) VALUES (999, 0.05)",
        )
        .execute(db.pool())
        .await
        .expect("insert customs office");

        let rate: f64 =
            sqlx::query_scalar("SELECT tax_rate FROM customs_offices WHERE planet_id = 999")
                .fetch_one(db.pool())
                .await
                .expect("fetch tax rate");
        assert!((rate - 0.05).abs() < f64::EPSILON);

        sqlx::query(
            "INSERT OR REPLACE INTO customs_offices (planet_id, tax_rate) VALUES (999, 0.10)",
        )
        .execute(db.pool())
        .await
        .expect("update customs office");

        let updated: f64 =
            sqlx::query_scalar("SELECT tax_rate FROM customs_offices WHERE planet_id = 999")
                .fetch_one(db.pool())
                .await
                .expect("fetch updated tax rate");
        assert!((updated - 0.10).abs() < f64::EPSILON);
    }
}
