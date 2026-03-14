use anyhow::Context as _;
use serde_json::{Value, json};
use sqlx::Row as _;

use super::EvepiService;
use crate::auth::store::TokenEntry;
use crate::db::sync_character;

pub async fn handle(svc: &EvepiService) -> anyhow::Result<Value> {
    // Collect all character IDs we have tokens for
    let rows = sqlx::query("SELECT character_id FROM characters")
        .fetch_all(svc.db.pool())
        .await
        .context("failed to load characters from db")?;

    let characters: Vec<i64> = rows
        .iter()
        .map(|r| r.try_get::<i64, _>("character_id"))
        .collect::<Result<_, _>>()
        .context("failed to read character_id column")?;

    let client_id =
        std::env::var("EVE_CLIENT_ID").context("EVE_CLIENT_ID environment variable not set")?;

    let mut synced = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for character_id in &characters {
        let key = format!("eve_char_{character_id}");
        let token_entry: TokenEntry = match svc.store.load(&key) {
            Ok(t) => t,
            Err(e) => {
                errors.push(format!("character {character_id}: no token ({e})"));
                continue;
            }
        };

        // Refresh token if needed (5-minute buffer)
        let access_token = if token_entry.expires_at < chrono::Utc::now().timestamp() + 300 {
            match crate::auth::eve_sso::refresh_eve_token(
                svc.esi.http_client(),
                &client_id,
                &token_entry.refresh_token,
            )
            .await
            {
                Ok(refreshed) => {
                    let _ = svc.store.save(&key, &refreshed);
                    refreshed.access_token
                }
                Err(e) => {
                    errors.push(format!(
                        "character {character_id}: token refresh failed ({e})"
                    ));
                    continue;
                }
            }
        } else {
            token_entry.access_token
        };

        match sync_character(&svc.db, &svc.esi, *character_id, &access_token).await {
            Ok(_) => synced += 1,
            Err(e) => errors.push(format!("character {character_id}: sync failed ({e})")),
        }
    }

    Ok(json!({
        "synced": synced,
        "total": characters.len(),
        "errors": errors,
    }))
}
