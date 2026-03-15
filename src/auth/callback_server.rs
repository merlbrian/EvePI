//! Background HTTP server that handles EVE SSO OAuth2 redirect callbacks.
//!
//! Started lazily on the first `start_eve_auth` MCP tool call. Binds to
//! `127.0.0.1:7878` — the same redirect URI already registered in the EVE
//! developer portal. The CLI `enroll` command uses the same port, so running
//! both simultaneously is intentionally prevented.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use anyhow::Context as _;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::auth::EveAuthSession;
use crate::auth::complete_eve_auth;
use crate::auth::enroll::{decode_eve_jwt, parse_callback};
use crate::auth::store::TokenStore;
use crate::db::Db;

/// Seconds before a pending auth entry is considered stale.
const STALE_SECONDS: i64 = 600;

/// Guard that ensures the callback HTTP server is started at most once per
/// MCP server process.
#[derive(Debug, Clone)]
pub struct CallbackServer {
    started: Arc<AtomicBool>,
}

impl CallbackServer {
    pub fn new() -> Self {
        Self {
            started: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Ensure the callback server is running. No-op if already started.
    ///
    /// Binds the TCP listener synchronously before spawning the accept loop,
    /// so any port-in-use error is returned to the caller immediately.
    pub async fn ensure_running(
        &self,
        db: Db,
        store: TokenStore,
        client_id: String,
    ) -> anyhow::Result<()> {
        if self
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let listener = match tokio::net::TcpListener::bind("127.0.0.1:7878").await {
                Ok(l) => l,
                Err(e) => {
                    // Reset so the next call can retry (e.g. after the port is freed).
                    self.started.store(false, Ordering::Release);
                    return Err(anyhow::anyhow!(
                        "failed to start EVE SSO callback server on port 7878: {e}\n\
                         If the CLI `enroll` command is running, wait for it to finish."
                    ));
                }
            };
            tokio::spawn(run_accept_loop(listener, db, store, client_id));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Internal implementation
// ---------------------------------------------------------------------------

async fn run_accept_loop(
    listener: tokio::net::TcpListener,
    db: Db,
    store: TokenStore,
    client_id: String,
) {
    // Clean up stale entries from any previous run.
    let _ = sqlx::query("DELETE FROM pending_auth WHERE created_at < datetime('now', ?1)")
        .bind(format!("-{STALE_SECONDS} seconds"))
        .execute(db.pool())
        .await;

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let db = db.clone();
                let store = store.clone();
                let client_id = client_id.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_callback(stream, db, store, client_id).await {
                        eprintln!("EVE SSO callback error: {e}");
                    }
                });
            }
            Err(e) => {
                eprintln!("EVE SSO callback server accept error: {e}");
            }
        }
    }
}

async fn handle_callback(
    mut stream: tokio::net::TcpStream,
    db: Db,
    store: TokenStore,
    client_id: String,
) -> anyhow::Result<()> {
    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .context("failed to read callback request")?;
    let raw = std::str::from_utf8(&buf[..n]).context("callback request was not valid UTF-8")?;

    // Ignore everything except GET /callback (e.g. favicon requests).
    if !raw.starts_with("GET /callback") {
        let resp = b"HTTP/1.1 204 No Content\r\n\r\n";
        let _ = stream.write_all(resp).await;
        return Ok(());
    }

    let (code, state) = match parse_callback(raw) {
        Ok(pair) => pair,
        Err(e) => {
            write_html(
                &mut stream,
                400,
                &error_html(&format!("Invalid callback: {e}")),
            )
            .await;
            return Ok(());
        }
    };

    // Look up the in-flight session, rejecting stale entries.
    let row: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT pkce_verifier, account_label, redirect_uri
         FROM pending_auth
         WHERE state = ?1
           AND created_at >= datetime('now', ?2)",
    )
    .bind(&state)
    .bind(format!("-{STALE_SECONDS} seconds"))
    .fetch_optional(db.pool())
    .await
    .context("pending_auth lookup failed")?;

    let (pkce_verifier, account_label, redirect_uri) = match row {
        Some(r) => r,
        None => {
            write_html(
                &mut stream,
                400,
                &error_html(
                    "Login session not found or expired. Please call start_eve_auth again.",
                ),
            )
            .await;
            return Ok(());
        }
    };

    // Exchange the code for tokens.
    let http = reqwest::Client::new();
    let session = EveAuthSession {
        pkce_verifier,
        state: state.clone(),
        redirect_uri,
    };
    let token_entry = match complete_eve_auth(&http, &client_id, &code, session).await {
        Ok(t) => t,
        Err(e) => {
            write_html(
                &mut stream,
                502,
                &error_html(&format!("EVE SSO token exchange failed: {e}")),
            )
            .await;
            return Ok(());
        }
    };

    // Decode character identity from the JWT.
    let (character_id, character_name) = match decode_character_identity(&token_entry.access_token)
    {
        Ok(id) => id,
        Err(e) => {
            write_html(
                &mut stream,
                500,
                &error_html(&format!("Failed to read character identity: {e}")),
            )
            .await;
            return Ok(());
        }
    };

    // Persist account + character to the database.
    let label = account_label.unwrap_or_else(|| character_name.clone());
    if let Err(e) = upsert_character(&db, character_id, &character_name, &label).await {
        write_html(
            &mut stream,
            500,
            &error_html(&format!("Database error: {e}")),
        )
        .await;
        return Ok(());
    }

    // Persist the token.
    let token_key = format!("eve_char_{character_id}");
    if let Err(e) = store.save(&token_key, &token_entry) {
        write_html(
            &mut stream,
            500,
            &error_html(&format!("Failed to save token: {e}")),
        )
        .await;
        return Ok(());
    }

    // Delete the consumed pending entry.
    let _ = sqlx::query("DELETE FROM pending_auth WHERE state = ?1")
        .bind(&state)
        .execute(db.pool())
        .await;

    write_html(&mut stream, 200, &success_html(&character_name)).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract `(character_id, character_name)` from an EVE SSO v2 JWT.
///
/// Delegates to the shared implementation in `enroll`.  The wrapper exists so
/// `callback_server` does not need to reach directly into the `enroll` module
/// for a function that is conceptually about JWT decoding.
fn decode_character_identity(access_token: &str) -> anyhow::Result<(i64, String)> {
    // Reuse the pub(crate) helper from enroll.rs
    decode_eve_jwt(access_token)
}

/// Upsert an account row and a character row into the database.
async fn upsert_character(
    db: &Db,
    character_id: i64,
    character_name: &str,
    account_label: &str,
) -> anyhow::Result<()> {
    sqlx::query("INSERT OR IGNORE INTO accounts (label) VALUES (?1)")
        .bind(account_label)
        .execute(db.pool())
        .await
        .context("upsert account")?;

    let account_id: i64 = sqlx::query_scalar("SELECT account_id FROM accounts WHERE label = ?1")
        .bind(account_label)
        .fetch_one(db.pool())
        .await
        .context("fetch account_id")?;

    sqlx::query(
        "INSERT INTO characters (character_id, account_id, character_name)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(character_id) DO UPDATE SET account_id = ?2, character_name = ?3",
    )
    .bind(character_id)
    .bind(account_id)
    .bind(character_name)
    .execute(db.pool())
    .await
    .context("upsert character")?;

    Ok(())
}

async fn write_html(stream: &mut tokio::net::TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        _ => "Unknown",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {len}\r\n\r\n{body}",
        len = body.len(),
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

fn success_html(character_name: &str) -> String {
    format!(
        "<!doctype html><html><head><title>EVE Auth</title></head><body>\
         <h1>&#x2713; Enrolled!</h1>\
         <p><strong>{character_name}</strong> has been added to EvePI.</p>\
         <p>You can close this tab. Run <code>sync_characters</code> in chat to pull colony data.</p>\
         </body></html>"
    )
}

fn error_html(message: &str) -> String {
    format!(
        "<!doctype html><html><head><title>EVE Auth Error</title></head><body>\
         <h1>&#x2717; Authorisation Failed</h1>\
         <p>{message}</p>\
         </body></html>"
    )
}

// Suppress unused-import warning: URL_SAFE_NO_PAD is used indirectly via decode_eve_jwt.
// The direct dependency on base64 here is used only through the enroll re-export.
#[allow(unused_imports)]
use base64::engine::general_purpose::URL_SAFE_NO_PAD as _;
