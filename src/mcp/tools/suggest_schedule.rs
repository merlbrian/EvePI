use anyhow::Context as _;
use serde_json::{Value, json};
use sqlx::Row as _;

use super::EvepiService;

pub async fn handle(svc: &EvepiService, character_id: Option<i64>) -> anyhow::Result<Value> {
    let rows = sqlx::query(
        r#"
        SELECT
            c.character_id,
            ch.character_name,
            c.planet_id,
            c.planet_type,
            c.planet_index,
            c.solar_system_id,
            ss.solar_system_name,
            ch.current_system_id,
            MIN(p.expiry_time) AS soonest_expiry,
            co.tax_rate
        FROM colonies c
        JOIN characters ch ON ch.character_id = c.character_id
        LEFT JOIN solar_systems ss ON ss.solar_system_id = c.solar_system_id
        LEFT JOIN pins p ON p.character_id = c.character_id
            AND p.planet_id = c.planet_id
            AND p.is_extractor = 1
            AND p.expiry_time IS NOT NULL
        LEFT JOIN customs_offices co ON co.planet_id = c.planet_id
        WHERE (?1 IS NULL OR c.character_id = ?1)
        GROUP BY c.character_id, c.planet_id
        HAVING soonest_expiry IS NOT NULL
        ORDER BY
            (ch.current_system_id = c.solar_system_id) DESC,
            soonest_expiry ASC
        "#,
    )
    .bind(character_id)
    .fetch_all(svc.db.pool())
    .await
    .context("suggest_schedule query failed")?;

    let schedule: Vec<Value> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| -> anyhow::Result<Value> {
            let cid: i64 = r.try_get("character_id")?;
            let char_name: Option<String> = r.try_get("character_name")?;
            let planet_id: i64 = r.try_get("planet_id")?;
            let planet_type: String = r.try_get("planet_type")?;
            let planet_index: i64 = r.try_get("planet_index")?;
            let solar_system_id: i64 = r.try_get("solar_system_id")?;
            let solar_system_name: Option<String> = r.try_get("solar_system_name")?;
            let current_system_id: Option<i64> = r.try_get("current_system_id")?;
            let soonest_expiry: Option<String> = r.try_get("soonest_expiry")?;
            let tax_rate: Option<f64> = r.try_get("tax_rate")?;

            let in_system = current_system_id == Some(solar_system_id);
            let label = format!(
                "{} {}",
                planet_type,
                crate::pi::to_roman(planet_index as u8)
            );
            Ok(json!({
                "priority": i + 1,
                "character_id": cid,
                "character_name": char_name,
                "planet_id": planet_id,
                "planet_label": label,
                "solar_system_name": solar_system_name,
                "soonest_expiry": soonest_expiry,
                "in_system": in_system,
                "travel_needed": !in_system,
                "poco_tax_rate": tax_rate.unwrap_or(0.0),
            }))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(json!({ "schedule": schedule }))
}
