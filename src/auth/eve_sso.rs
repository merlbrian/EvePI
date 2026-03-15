use anyhow::Context as _;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Scopes required for full PI read access + character location.
pub const EVE_SCOPES: &[&str] = &[
    "esi-planets.manage_planets.v1",
    "esi-location.read_location.v1",
];

const EVE_AUTH_URL: &str = "https://login.eveonline.com/v2/oauth/authorize";
const EVE_TOKEN_URL: &str = "https://login.eveonline.com/v2/oauth/token";

/// In-flight state for a PKCE auth session.
pub struct EveAuthSession {
    pub pkce_verifier: String,
    pub state: String,
    pub redirect_uri: String,
}

/// Generate the EVE SSO authorization URL and return the in-flight session.
/// The caller must open the returned URL in a browser.
pub fn begin_eve_auth(
    client_id: &str,
    redirect_uri: &str,
) -> anyhow::Result<(String, EveAuthSession)> {
    let verifier = pkce_verifier();
    let challenge = pkce_challenge(&verifier);
    let state = random_state();

    let scope = EVE_SCOPES.join(" ");
    let url = format!(
        "{auth}?response_type=code\
         &client_id={client_id}\
         &redirect_uri={redirect}\
         &scope={scope}\
         &state={state}\
         &code_challenge={challenge}\
         &code_challenge_method=S256",
        auth = EVE_AUTH_URL,
        client_id = client_id,
        redirect = urlencoding::encode(redirect_uri),
        scope = urlencoding::encode(&scope),
        state = &state,
        challenge = &challenge,
    );

    let session = EveAuthSession {
        pkce_verifier: verifier,
        state,
        redirect_uri: redirect_uri.to_owned(),
    };
    Ok((url, session))
}

/// Exchange the authorization code for tokens using the PKCE verifier.
pub async fn complete_eve_auth(
    client: &reqwest::Client,
    client_id: &str,
    code: &str,
    session: EveAuthSession,
) -> anyhow::Result<crate::auth::store::TokenEntry> {
    let body = [
        ("grant_type", "authorization_code"),
        ("client_id", client_id),
        ("code", code),
        ("redirect_uri", &session.redirect_uri),
        ("code_verifier", &session.pkce_verifier),
    ];

    let resp = client
        .post(EVE_TOKEN_URL)
        .form(&body)
        .send()
        .await
        .context("EVE SSO token request failed")?
        .error_for_status()
        .context("EVE SSO token endpoint returned error")?;

    let raw: serde_json::Value = resp.json().await.context("EVE SSO token parse failed")?;
    Ok(crate::auth::store::TokenEntry {
        access_token: raw["access_token"]
            .as_str()
            .context("missing access_token")?
            .to_owned(),
        refresh_token: raw["refresh_token"]
            .as_str()
            .context("missing refresh_token")?
            .to_owned(),
        expires_at: chrono::Utc::now().timestamp() + raw["expires_in"].as_i64().unwrap_or(1200),
    })
}

/// Refresh an expired EVE access token.
pub async fn refresh_eve_token(
    client: &reqwest::Client,
    client_id: &str,
    refresh_token: &str,
) -> anyhow::Result<crate::auth::store::TokenEntry> {
    let body = [
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("refresh_token", refresh_token),
    ];

    let resp = client
        .post(EVE_TOKEN_URL)
        .form(&body)
        .send()
        .await
        .context("EVE SSO refresh request failed")?
        .error_for_status()
        .context("EVE SSO refresh endpoint returned error")?;

    let raw: serde_json::Value = resp.json().await.context("EVE SSO refresh parse failed")?;
    Ok(crate::auth::store::TokenEntry {
        access_token: raw["access_token"]
            .as_str()
            .context("missing access_token")?
            .to_owned(),
        refresh_token: raw["refresh_token"]
            .as_str()
            .unwrap_or(refresh_token)
            .to_owned(),
        expires_at: chrono::Utc::now().timestamp() + raw["expires_in"].as_i64().unwrap_or(1200),
    })
}

// --- PKCE helpers ---

fn pkce_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

fn random_state() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
