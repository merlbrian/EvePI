use anyhow::Context as _;
use serde_json::{Value, json};
use sqlx::Row as _;

use super::EvepiService;

pub async fn handle(
    svc: &EvepiService,
    character_id: i64,
    planet_id: i64,
) -> anyhow::Result<Value> {
    let colony_row = sqlx::query(
        r#"
        SELECT c.planet_type, c.planet_index, c.solar_system_id,
               c.upgrade_level, c.num_pins, c.last_update,
               ss.solar_system_name
        FROM colonies c
        LEFT JOIN solar_systems ss ON ss.solar_system_id = c.solar_system_id
        WHERE c.character_id = ?1 AND c.planet_id = ?2
        "#,
    )
    .bind(character_id)
    .bind(planet_id)
    .fetch_optional(svc.db.pool())
    .await
    .context("get_colony_layout colony query failed")?
    .ok_or_else(|| {
        anyhow::anyhow!("colony not found for character {character_id} planet {planet_id}; run sync_characters first")
    })?;

    let planet_type: String = colony_row.try_get("planet_type")?;
    let planet_index: i64 = colony_row.try_get("planet_index")?;
    let solar_system_name: Option<String> = colony_row.try_get("solar_system_name")?;
    let upgrade_level: i64 = colony_row.try_get("upgrade_level")?;
    let num_pins: i64 = colony_row.try_get("num_pins")?;
    let last_update: Option<String> = colony_row.try_get("last_update")?;

    let pin_rows = sqlx::query(
        r#"
        SELECT pin_id, type_id, is_extractor, schematic_id, expiry_time, install_time
        FROM pins
        WHERE character_id = ?1 AND planet_id = ?2
        "#,
    )
    .bind(character_id)
    .bind(planet_id)
    .fetch_all(svc.db.pool())
    .await
    .context("get_colony_layout pins query failed")?;

    let route_rows = sqlx::query(
        r#"
        SELECT route_id, source_pin_id, destination_pin_id,
               content_type_id, quantity
        FROM routes
        WHERE character_id = ?1 AND planet_id = ?2
        "#,
    )
    .bind(character_id)
    .bind(planet_id)
    .fetch_all(svc.db.pool())
    .await
    .context("get_colony_layout routes query failed")?;

    let label = format!(
        "{} {}",
        planet_type,
        crate::pi::to_roman(planet_index as u8)
    );

    let pins: Vec<Value> = pin_rows
        .iter()
        .map(|p| -> anyhow::Result<Value> {
            Ok(json!({
                "pin_id": p.try_get::<i64, _>("pin_id")?,
                "type_id": p.try_get::<i64, _>("type_id")?,
                "is_extractor": p.try_get::<i64, _>("is_extractor")? != 0,
                "schematic_id": p.try_get::<Option<i64>, _>("schematic_id")?,
                "expiry_time": p.try_get::<Option<String>, _>("expiry_time")?,
                "install_time": p.try_get::<Option<String>, _>("install_time")?,
            }))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let routes: Vec<Value> = route_rows
        .iter()
        .map(|r| -> anyhow::Result<Value> {
            Ok(json!({
                "route_id": r.try_get::<i64, _>("route_id")?,
                "source_pin_id": r.try_get::<i64, _>("source_pin_id")?,
                "destination_pin_id": r.try_get::<i64, _>("destination_pin_id")?,
                "content_type_id": r.try_get::<i64, _>("content_type_id")?,
                "quantity": r.try_get::<f64, _>("quantity")?,
            }))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(json!({
        "character_id": character_id,
        "planet_id": planet_id,
        "planet_label": label,
        "solar_system_name": solar_system_name,
        "upgrade_level": upgrade_level,
        "num_pins": num_pins,
        "last_update": last_update,
        "pins": pins,
        "routes": routes,
    }))
}
