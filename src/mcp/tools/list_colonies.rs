use anyhow::Context as _;
use serde_json::{Value, json};
use sqlx::Row as _;

use super::EvepiService;

pub async fn handle(
    svc: &EvepiService,
    character_id: Option<i64>,
    account_id: Option<i64>,
) -> anyhow::Result<Value> {
    let rows = sqlx::query(
        r#"
        SELECT
            c.character_id,
            ch.character_name,
            c.planet_id,
            c.planet_type,
            c.planet_index,
            c.solar_system_id,
            c.upgrade_level,
            c.num_pins,
            c.last_update,
            ch.current_system_id,
            ss.solar_system_name,
            MIN(p.expiry_time) AS soonest_expiry
        FROM colonies c
        JOIN characters ch ON ch.character_id = c.character_id
        LEFT JOIN solar_systems ss ON ss.solar_system_id = c.solar_system_id
        LEFT JOIN pins p ON p.character_id = c.character_id
            AND p.planet_id = c.planet_id
            AND p.is_extractor = 1
        WHERE (?1 IS NULL OR c.character_id = ?1)
          AND (?2 IS NULL OR ch.account_id = ?2)
        GROUP BY c.character_id, c.planet_id
        ORDER BY ch.character_name, c.planet_type, c.planet_index
        "#,
    )
    .bind(character_id)
    .bind(account_id)
    .fetch_all(svc.db.pool())
    .await
    .context("list_colonies query failed")?;

    let colonies: Vec<Value> = rows
        .iter()
        .map(|r| -> anyhow::Result<Value> {
            let cid: i64 = r.try_get("character_id")?;
            let char_name: Option<String> = r.try_get("character_name")?;
            let planet_id: i64 = r.try_get("planet_id")?;
            let planet_type: String = r.try_get("planet_type")?;
            let planet_index: i64 = r.try_get("planet_index")?;
            let solar_system_id: i64 = r.try_get("solar_system_id")?;
            let upgrade_level: i64 = r.try_get("upgrade_level")?;
            let num_pins: i64 = r.try_get("num_pins")?;
            let last_update: Option<String> = r.try_get("last_update")?;
            let current_system_id: Option<i64> = r.try_get("current_system_id")?;
            let solar_system_name: Option<String> = r.try_get("solar_system_name")?;
            let soonest_expiry: Option<String> = r.try_get("soonest_expiry")?;

            // Fall back to home_system_id when ESI location hasn't been synced yet.
            let effective_system = current_system_id.or(svc.home_system_id);
            let travel_needed = effective_system != Some(solar_system_id);
            let label = format!(
                "{} {}",
                planet_type,
                crate::pi::to_roman(planet_index as u8)
            );
            Ok(json!({
                "character_id": cid,
                "character_name": char_name,
                "planet_id": planet_id,
                "planet_label": label,
                "solar_system_id": solar_system_id,
                "solar_system_name": solar_system_name,
                "upgrade_level": upgrade_level,
                "num_pins": num_pins,
                "soonest_expiry": soonest_expiry,
                "last_update": last_update,
                "travel_needed": travel_needed,
            }))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(json!({ "colonies": colonies }))
}
