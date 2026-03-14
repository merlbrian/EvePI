use anyhow::Context as _;

use crate::db::Db;
use crate::esi::EsiClient;
use crate::pi::CharacterId;

/// Pull all colonies and current location for `character_id` from ESI and
/// upsert into the database.
pub async fn sync_character(
    db: &Db,
    esi: &EsiClient,
    character_id: CharacterId,
    access_token: &str,
) -> anyhow::Result<()> {
    // Fetch colonies list
    let colonies = esi
        .list_colonies(character_id, access_token)
        .await
        .context("failed to list colonies")?;

    // Fetch current location
    let location = esi
        .get_character_location(character_id, access_token)
        .await
        .context("failed to get character location")?;

    // Upsert character location
    sqlx::query(
        r#"
        INSERT INTO characters (character_id, current_system_id, last_synced_at)
        VALUES (?1, ?2, CURRENT_TIMESTAMP)
        ON CONFLICT(character_id) DO UPDATE SET
            current_system_id = excluded.current_system_id,
            last_synced_at    = excluded.last_synced_at
        "#,
    )
    .bind(character_id)
    .bind(location.solar_system_id)
    .execute(db.pool())
    .await
    .context("upsert character failed")?;

    for colony in &colonies {
        // Upsert colony summary
        sqlx::query(
            r#"
            INSERT INTO colonies
                (character_id, planet_id, planet_type, solar_system_id,
                 upgrade_level, num_pins, last_update, last_synced_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)
            ON CONFLICT(character_id, planet_id) DO UPDATE SET
                planet_type    = excluded.planet_type,
                solar_system_id= excluded.solar_system_id,
                upgrade_level  = excluded.upgrade_level,
                num_pins       = excluded.num_pins,
                last_update    = excluded.last_update,
                last_synced_at = excluded.last_synced_at
            "#,
        )
        .bind(character_id)
        .bind(colony.planet_id)
        .bind(&colony.planet_type)
        .bind(colony.solar_system_id)
        .bind(colony.upgrade_level)
        .bind(colony.num_pins)
        .bind(&colony.last_update)
        .execute(db.pool())
        .await
        .context("upsert colony failed")?;

        // Fetch and persist full layout (pins, routes)
        let layout = esi
            .get_colony_layout(character_id, colony.planet_id, access_token)
            .await
            .context("failed to get colony layout")?;

        // Delete stale pins/routes for this colony and re-insert
        sqlx::query("DELETE FROM pins WHERE character_id = ?1 AND planet_id = ?2")
            .bind(character_id)
            .bind(colony.planet_id)
            .execute(db.pool())
            .await?;

        sqlx::query("DELETE FROM routes WHERE character_id = ?1 AND planet_id = ?2")
            .bind(character_id)
            .bind(colony.planet_id)
            .execute(db.pool())
            .await?;

        for pin in &layout.pins {
            let (is_extractor, schematic_id, expiry_time, install_time) =
                if let Some(ext) = &pin.extractor_details {
                    let _ = ext; // heads processed below
                    (
                        true,
                        None::<i64>,
                        pin.expiry_time.clone(),
                        pin.install_time.clone(),
                    )
                } else {
                    (false, pin.schematic_id.map(|s| s as i64), None, None)
                };
            let is_extractor_int = i64::from(is_extractor);

            sqlx::query(
                r#"
                INSERT INTO pins
                    (pin_id, character_id, planet_id, type_id, is_extractor,
                     schematic_id, expiry_time, install_time)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
            )
            .bind(pin.pin_id)
            .bind(character_id)
            .bind(colony.planet_id)
            .bind(pin.type_id)
            .bind(is_extractor_int)
            .bind(schematic_id)
            .bind(&expiry_time)
            .bind(&install_time)
            .execute(db.pool())
            .await
            .context("insert pin failed")?;

            // Insert extractor heads if present
            if let Some(ext) = &pin.extractor_details {
                for head in &ext.heads {
                    let head_id = head.head_id as i64;
                    sqlx::query(
                        r#"
                        INSERT INTO extractor_heads
                            (pin_id, head_id, latitude, longitude)
                        VALUES (?1, ?2, ?3, ?4)
                        "#,
                    )
                    .bind(pin.pin_id)
                    .bind(head_id)
                    .bind(head.latitude)
                    .bind(head.longitude)
                    .execute(db.pool())
                    .await
                    .context("insert extractor head failed")?;
                }
            }
        }

        for route in &layout.routes {
            sqlx::query(
                r#"
                INSERT INTO routes
                    (route_id, character_id, planet_id, source_pin_id,
                     destination_pin_id, content_type_id, quantity)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )
            .bind(route.route_id)
            .bind(character_id)
            .bind(colony.planet_id)
            .bind(route.source_pin_id)
            .bind(route.destination_pin_id)
            .bind(route.content_type_id)
            .bind(route.quantity)
            .execute(db.pool())
            .await
            .context("insert route failed")?;
        }
    }

    Ok(())
}
