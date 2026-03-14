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
        SELECT p.pin_id, p.type_id, p.is_extractor, p.schematic_id,
               p.expiry_time, p.install_time, p.product_type_id,
               pt.type_name  AS product_type_name,
               s.schematic_name
        FROM pins p
        LEFT JOIN eve_types  pt ON pt.type_id      = p.product_type_id
        LEFT JOIN schematics s  ON s.schematic_id  = p.schematic_id
        WHERE p.character_id = ?1 AND p.planet_id = ?2
        "#,
    )
    .bind(character_id)
    .bind(planet_id)
    .fetch_all(svc.db.pool())
    .await
    .context("get_colony_layout pins query failed")?;

    let route_rows = sqlx::query(
        r#"
        SELECT r.route_id, r.source_pin_id, r.destination_pin_id,
               r.content_type_id, r.quantity,
               et.type_name AS content_type_name
        FROM routes r
        LEFT JOIN eve_types et ON et.type_id = r.content_type_id
        WHERE r.character_id = ?1 AND r.planet_id = ?2
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
                "product_type_id": p.try_get::<Option<i64>, _>("product_type_id")?,
                "product_type_name": p.try_get::<Option<String>, _>("product_type_name")?,
                "schematic_id": p.try_get::<Option<i64>, _>("schematic_id")?,
                "schematic_name": p.try_get::<Option<String>, _>("schematic_name")?,
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
                "content_type_name": r.try_get::<Option<String>, _>("content_type_name")?,
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
