//! Interactive character enrolment via EVE SSO.
//!
//! Spins up a local HTTP listener on port 7878, prints the authorization URL
//! for the user to open, captures the OAuth callback, exchanges the code for
//! tokens, and persists the character + token to the DB and token store.

use anyhow::Context as _;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::auth::store::TokenStore;
use crate::auth::{begin_eve_auth, complete_eve_auth};
use crate::db::Db;

const REDIRECT_URI: &str = "http://localhost:7878/callback";

/// Run the interactive character enrolment flow.
///
/// Starts a local HTTP listener, prints the EVE SSO authorization URL, waits
/// for the browser redirect, exchanges the code for tokens, decodes the
/// character identity from the EVE SSO JWT, and persists everything to the DB
/// and token store.
pub async fn run_enroll(account_label: Option<String>) -> anyhow::Result<()> {
    let client_id =
        std::env::var("EVE_CLIENT_ID").context("EVE_CLIENT_ID environment variable not set")?;

    // Bind first so we fail fast if the port is already in use.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:7878")
        .await
        .context("failed to bind port 7878 — is another process using it?")?;

    let (url, session) = begin_eve_auth(&client_id, REDIRECT_URI)?;

    println!("Open this URL in your browser to authorise the character:\n");
    println!("  {url}\n");
    println!("Waiting for the OAuth callback on http://localhost:7878/callback …");

    // Wait for the browser to hit the redirect URI.
    let (mut stream, _) = listener
        .accept()
        .await
        .context("failed to accept OAuth callback connection")?;

    // Read the raw HTTP GET request (the first line is all we need).
    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .context("failed to read callback request")?;
    let raw = std::str::from_utf8(&buf[..n]).context("callback request was not valid UTF-8")?;

    // Respond immediately so the browser shows a success page.
    let html = b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
        <!doctype html><html><body>\
        <h1>Authorised!</h1>\
        <p>You can close this tab and return to the terminal.</p>\
        </body></html>";
    stream
        .write_all(html)
        .await
        .context("failed to write callback response")?;
    drop(stream);
    drop(listener);

    let (code, state) = parse_callback(raw)?;
    if state != session.state {
        anyhow::bail!("OAuth state mismatch — possible CSRF; aborting enrolment");
    }

    let http = reqwest::Client::new();
    let token_entry = complete_eve_auth(&http, &client_id, &code, session)
        .await
        .context("EVE SSO token exchange failed")?;

    let (character_id, character_name) = decode_eve_jwt(&token_entry.access_token)
        .context("failed to decode character identity from EVE SSO token")?;

    let label = resolve_label(account_label, &character_name).await?;

    // Persist account + character to the DB.
    let db = Db::open().await.context("failed to open database")?;

    sqlx::query("INSERT OR IGNORE INTO accounts (label) VALUES (?1)")
        .bind(&label)
        .execute(db.pool())
        .await
        .context("upsert account")?;

    let account_id: i64 =
        sqlx::query_scalar("SELECT account_id FROM accounts WHERE label = ?1")
            .bind(&label)
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
    .bind(&character_name)
    .execute(db.pool())
    .await
    .context("upsert character")?;

    // Persist the token.
    let store = TokenStore::new().context("failed to open token store")?;
    let token_key = format!("eve_char_{character_id}");
    store
        .save(&token_key, &token_entry)
        .context("failed to save token")?;

    println!(
        "\n✓  Enrolled {} (ID {}) under account '{}'",
        character_name, character_id, label
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Prompt for an account label if none was supplied on the CLI.
async fn resolve_label(
    supplied: Option<String>,
    character_name: &str,
) -> anyhow::Result<String> {
    if let Some(label) = supplied {
        return Ok(label);
    }
    let prompt = format!("Enter account label [default: {}]: ", character_name);
    let default = character_name.to_owned();
    tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        use std::io::Write as _;
        print!("{prompt}");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        Ok(if trimmed.is_empty() {
            default
        } else {
            trimmed.to_owned()
        })
    })
    .await
    .context("label prompt task failed")?
}

/// Extract `(character_id, character_name)` from an EVE SSO v2 JWT.
///
/// The `sub` claim has the form `"CHARACTER:EVE:12345678"`. The signature is
/// not verified — the token was received directly from the EVE SSO token
/// endpoint over TLS, so origin trust is already established.
fn decode_eve_jwt(token: &str) -> anyhow::Result<(i64, String)> {
    let payload_b64 = token.split('.').nth(1).context("JWT has no payload segment")?;
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .context("JWT payload base64 decode failed")?;
    let payload: serde_json::Value =
        serde_json::from_slice(&payload_bytes).context("JWT payload JSON parse failed")?;

    let sub = payload["sub"].as_str().context("JWT missing `sub` claim")?;
    let character_id: i64 = sub
        .split(':')
        .nth(2)
        .context("unexpected `sub` format (expected CHARACTER:EVE:<id>)")?
        .parse()
        .context("character ID in `sub` is not a valid integer")?;

    let character_name = payload["name"]
        .as_str()
        .context("JWT missing `name` claim")?
        .to_owned();

    Ok((character_id, character_name))
}

/// Parse the OAuth `code` and `state` query parameters from a raw HTTP GET
/// request line (e.g. `GET /callback?code=abc&state=xyz HTTP/1.1`).
fn parse_callback(request: &str) -> anyhow::Result<(String, String)> {
    let first_line = request.lines().next().context("empty callback request")?;
    let path = first_line
        .split_whitespace()
        .nth(1)
        .context("malformed HTTP request line")?;

    let query = path
        .split_once('?')
        .map(|(_, q)| q)
        .context("no query string in callback URL")?;

    let mut code = None;
    let mut state = None;
    for param in query.split('&') {
        if let Some(v) = param.strip_prefix("code=") {
            code = Some(urlencoding::decode(v).context("decode `code`")?.into_owned());
        } else if let Some(v) = param.strip_prefix("state=") {
            state = Some(
                urlencoding::decode(v)
                    .context("decode `state`")?
                    .into_owned(),
            );
        }
    }

    Ok((
        code.context("missing `code` in callback query string")?,
        state.context("missing `state` in callback query string")?,
    ))
}
