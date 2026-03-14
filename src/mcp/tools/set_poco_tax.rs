use anyhow::Context as _;
use serde_json::{Value, json};

use super::EvepiService;

pub async fn handle(svc: &EvepiService, planet_id: i64, tax_rate: f64) -> anyhow::Result<Value> {
    anyhow::ensure!(
        (0.0..=1.0).contains(&tax_rate),
        "tax_rate must be between 0.0 and 1.0"
    );

    sqlx::query(
        r#"
        INSERT INTO customs_offices (planet_id, tax_rate, from_esi, updated_at)
        VALUES (?1, ?2, 0, CURRENT_TIMESTAMP)
        ON CONFLICT(planet_id) DO UPDATE SET
            tax_rate   = excluded.tax_rate,
            from_esi   = 0,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(planet_id)
    .bind(tax_rate)
    .execute(svc.db.pool())
    .await
    .context("set_poco_tax upsert failed")?;

    Ok(json!({
        "planet_id": planet_id,
        "tax_rate": tax_rate,
        "message": format!("POCO tax for planet {} set to {:.1}%", planet_id, tax_rate * 100.0),
    }))
}
