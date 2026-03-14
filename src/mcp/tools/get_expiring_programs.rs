use anyhow::Context as _;
use serde_json::{Value, json};
use sqlx::Row as _;

use super::EvepiService;

pub async fn handle(
    svc: &EvepiService,
    hours: i64,
    character_id: Option<i64>,
) -> anyhow::Result<Value> {
    let rows = sqlx::query(
        r#"
        SELECT
            p.pin_id,
            p.expiry_time,
            p.install_time,
            c.character_id,
            ch.character_name,
            c.planet_id,
            c.planet_type,
            c.planet_index,
            c.solar_system_id,
            ch.current_system_id,
            ss.solar_system_name
        FROM pins p
        JOIN colonies c ON c.character_id = p.character_id AND c.planet_id = p.planet_id
        JOIN characters ch ON ch.character_id = c.character_id
        LEFT JOIN solar_systems ss ON ss.solar_system_id = c.solar_system_id
        WHERE p.is_extractor = 1
          AND p.expiry_time IS NOT NULL
          AND p.expiry_time <= datetime('now', '+' || ?1 || ' hours')
          AND (?2 IS NULL OR c.character_id = ?2)
        ORDER BY p.expiry_time ASC
        "#,
    )
    .bind(hours)
    .bind(character_id)
    .fetch_all(svc.db.pool())
    .await
    .context("get_expiring_programs query failed")?;

    let programs: Vec<Value> = rows
        .iter()
        .map(|r| -> anyhow::Result<Value> {
            let cid: i64 = r.try_get("character_id")?;
            let char_name: Option<String> = r.try_get("character_name")?;
            let planet_id: i64 = r.try_get("planet_id")?;
            let planet_type: String = r.try_get("planet_type")?;
            let planet_index: i64 = r.try_get("planet_index")?;
            let solar_system_id: i64 = r.try_get("solar_system_id")?;
            let current_system_id: Option<i64> = r.try_get("current_system_id")?;
            let solar_system_name: Option<String> = r.try_get("solar_system_name")?;
            let expiry_time: Option<String> = r.try_get("expiry_time")?;
            let install_time: Option<String> = r.try_get("install_time")?;

            let travel_needed = current_system_id != Some(solar_system_id);
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
                "solar_system_name": solar_system_name,
                "expiry_time": expiry_time,
                "install_time": install_time,
                "travel_needed": travel_needed,
            }))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(json!({
        "hours_window": hours,
        "expiring": programs,
    }))
}
