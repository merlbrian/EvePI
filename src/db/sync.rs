use anyhow::Context as _;

use crate::db::Db;
use crate::esi::EsiClient;
use crate::pi::CharacterId;

// ---------------------------------------------------------------------------
// Cache helpers — check the DB first, hit ESI only on a miss
// ---------------------------------------------------------------------------

async fn ensure_solar_system_cached(db: &Db, esi: &EsiClient, id: i64) -> anyhow::Result<()> {
    if sqlx::query("SELECT 1 FROM solar_systems WHERE solar_system_id = ?1")
        .bind(id)
        .fetch_optional(db.pool())
        .await?
        .is_some()
    {
        return Ok(());
    }
    let sys = esi
        .get_solar_system(id)
        .await
        .with_context(|| format!("fetch solar system {id}"))?;
    sqlx::query(
        "INSERT OR IGNORE INTO solar_systems (solar_system_id, solar_system_name) VALUES (?1, ?2)",
    )
    .bind(sys.system_id)
    .bind(&sys.name)
    .execute(db.pool())
    .await
    .context("cache solar_system")?;
    Ok(())
}

async fn ensure_type_cached(db: &Db, esi: &EsiClient, type_id: i64) -> anyhow::Result<()> {
    if sqlx::query("SELECT 1 FROM eve_types WHERE type_id = ?1")
        .bind(type_id)
        .fetch_optional(db.pool())
        .await?
        .is_some()
    {
        return Ok(());
    }
    let t = esi
        .get_type_name(type_id)
        .await
        .with_context(|| format!("fetch type {type_id}"))?;
    sqlx::query("INSERT OR IGNORE INTO eve_types (type_id, type_name) VALUES (?1, ?2)")
        .bind(t.type_id)
        .bind(&t.name)
        .execute(db.pool())
        .await
        .context("cache eve_type")?;
    Ok(())
}

/// Cache a schematic's name, cycle time, and all input/output type definitions.
/// Also caches type names for every schematic pin resource.
async fn ensure_schematic_cached(
    db: &Db,
    esi: &EsiClient,
    schematic_id: i64,
) -> anyhow::Result<()> {
    if sqlx::query("SELECT 1 FROM schematics WHERE schematic_id = ?1")
        .bind(schematic_id)
        .fetch_optional(db.pool())
        .await?
        .is_some()
    {
        return Ok(());
    }
    let s = esi
        .get_schematic(schematic_id as u32)
        .await
        .with_context(|| format!("fetch schematic {schematic_id}"))?;
    sqlx::query(
        "INSERT OR IGNORE INTO schematics (schematic_id, schematic_name, cycle_time) \
         VALUES (?1, ?2, ?3)",
    )
    .bind(schematic_id)
    .bind(&s.schematic_name)
    .bind(s.cycle_time as i64)
    .execute(db.pool())
    .await
    .context("cache schematic")?;

    for pin in &s.pins {
        // Cache each resource type referenced by this schematic (best-effort).
        let _ = ensure_type_cached(db, esi, pin.type_id).await;
        if pin.is_input {
            sqlx::query(
                "INSERT OR IGNORE INTO schematic_inputs (schematic_id, type_id, quantity) \
                 VALUES (?1, ?2, ?3)",
            )
            .bind(schematic_id)
            .bind(pin.type_id)
            .bind(pin.quantity as i64)
            .execute(db.pool())
            .await
            .context("cache schematic_input")?;
        } else {
            sqlx::query(
                "INSERT OR IGNORE INTO schematic_output (schematic_id, type_id, quantity) \
                 VALUES (?1, ?2, ?3)",
            )
            .bind(schematic_id)
            .bind(pin.type_id)
            .bind(pin.quantity as i64)
            .execute(db.pool())
            .await
            .context("cache schematic_output")?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Planet index assignment
// ---------------------------------------------------------------------------

/// Assign stable 1-based indices within each (solar_system_id, planet_type)
/// group ordered by planet_id. Drives "Gas III", "Lava I" display labels.
/// Must be called after all colonies for the character have been upserted.
async fn assign_planet_indices(db: &Db, character_id: CharacterId) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE colonies
        SET planet_index = (
            SELECT COUNT(*)
            FROM colonies c2
            WHERE c2.character_id  = colonies.character_id
              AND c2.solar_system_id = colonies.solar_system_id
              AND c2.planet_type    = colonies.planet_type
              AND c2.planet_id     <= colonies.planet_id
        )
        WHERE character_id = ?1
        "#,
    )
    .bind(character_id)
    .execute(db.pool())
    .await
    .context("assign planet indices")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Capitalise the first character of a string slice.
/// ESI returns planet types lower-case ("gas", "lava"); we store "Gas", "Lava".
fn capitalise_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

// ---------------------------------------------------------------------------
// Main sync entry point
// ---------------------------------------------------------------------------

/// Pull all colonies and current location for `character_id` from ESI, persist
/// them to the database, and enrich with cached reference data (solar system
/// names, schematic definitions, type names).
///
/// Planet indices ("Gas III", "Lava I") are assigned after all colonies are
/// upserted so the numbering is stable across a character's full set of planets.
///
/// Reference data lookups (solar system names, type names, schematics) are
/// best-effort — a lookup failure does not abort the sync.
pub async fn sync_character(
    db: &Db,
    esi: &EsiClient,
    character_id: CharacterId,
    access_token: &str,
) -> anyhow::Result<()> {
    let colonies = esi
        .list_colonies(character_id, access_token)
        .await
        .context("list colonies")?;

    let location = esi
        .get_character_location(character_id, access_token)
        .await
        .context("get character location")?;

    // Cache the character's current system name (best-effort).
    let _ = ensure_solar_system_cached(db, esi, location.solar_system_id).await;

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
    .context("upsert character")?;

    for colony in &colonies {
        // Cache system name — likely same as character's location for all colonies.
        let _ = ensure_solar_system_cached(db, esi, colony.solar_system_id).await;

        // ESI returns planet_type lower-case ("gas", "lava"); capitalise on store.
        let planet_type = capitalise_first(&colony.planet_type);

        sqlx::query(
            r#"
            INSERT INTO colonies
                (character_id, planet_id, planet_type, solar_system_id,
                 upgrade_level, num_pins, last_update, last_synced_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)
            ON CONFLICT(character_id, planet_id) DO UPDATE SET
                planet_type     = excluded.planet_type,
                solar_system_id = excluded.solar_system_id,
                upgrade_level   = excluded.upgrade_level,
                num_pins        = excluded.num_pins,
                last_update     = excluded.last_update,
                last_synced_at  = excluded.last_synced_at
            "#,
        )
        .bind(character_id)
        .bind(colony.planet_id)
        .bind(&planet_type)
        .bind(colony.solar_system_id)
        .bind(colony.upgrade_level)
        .bind(colony.num_pins)
        .bind(&colony.last_update)
        .execute(db.pool())
        .await
        .context("upsert colony")?;

        let layout = esi
            .get_colony_layout(character_id, colony.planet_id, access_token)
            .await
            .context("get colony layout")?;

        // Remove stale extractor_heads first — no CASCADE constraint in schema.
        sqlx::query(
            r#"
            DELETE FROM extractor_heads
            WHERE pin_id IN (
                SELECT pin_id FROM pins WHERE character_id = ?1 AND planet_id = ?2
            )
            "#,
        )
        .bind(character_id)
        .bind(colony.planet_id)
        .execute(db.pool())
        .await?;

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
            let (is_extractor, schematic_id, product_type_id, expiry_time, install_time) =
                if let Some(ext) = &pin.extractor_details {
                    // Cache the extracted resource type (best-effort).
                    if let Some(pid) = ext.product_type_id {
                        let _ = ensure_type_cached(db, esi, pid).await;
                    }
                    (
                        true,
                        None::<i64>,
                        ext.product_type_id,
                        pin.expiry_time.clone(),
                        pin.install_time.clone(),
                    )
                } else {
                    // Cache schematic definition + all referenced type names (best-effort).
                    if let Some(sid) = pin.schematic_id {
                        let _ = ensure_schematic_cached(db, esi, sid as i64).await;
                    }
                    (false, pin.schematic_id.map(|s| s as i64), None, None, None)
                };

            sqlx::query(
                r#"
                INSERT INTO pins
                    (pin_id, character_id, planet_id, type_id, is_extractor,
                     schematic_id, product_type_id, expiry_time, install_time)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
            )
            .bind(pin.pin_id)
            .bind(character_id)
            .bind(colony.planet_id)
            .bind(pin.type_id)
            .bind(i64::from(is_extractor))
            .bind(schematic_id)
            .bind(product_type_id)
            .bind(&expiry_time)
            .bind(&install_time)
            .execute(db.pool())
            .await
            .context("insert pin")?;

            if let Some(ext) = &pin.extractor_details {
                for head in &ext.heads {
                    sqlx::query(
                        "INSERT INTO extractor_heads \
                         (pin_id, head_id, latitude, longitude) VALUES (?1, ?2, ?3, ?4)",
                    )
                    .bind(pin.pin_id)
                    .bind(head.head_id as i64)
                    .bind(head.latitude)
                    .bind(head.longitude)
                    .execute(db.pool())
                    .await
                    .context("insert extractor_head")?;
                }
            }
        }

        for route in &layout.routes {
            // Cache the routed resource type (best-effort).
            let _ = ensure_type_cached(db, esi, route.content_type_id).await;

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
            .context("insert route")?;
        }
    }

    // Assign planet indices now that all colonies for this character are upserted.
    assign_planet_indices(db, character_id)
        .await
        .context("assign planet indices")?;

    Ok(())
}
