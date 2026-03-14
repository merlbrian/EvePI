use anyhow::Context as _;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;

const GITHUB_AUTH_URL: &str = "https://github.com/login/oauth/authorize";
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

pub struct GitHubAuthSession {
    pub state: String,
    pub redirect_uri: String,
}

/// Generate the GitHub OAuth authorization URL.
pub fn begin_github_auth(client_id: &str, redirect_uri: &str) -> (String, GitHubAuthSession) {
    let state = random_state();
    let url = format!(
        "{auth}?client_id={client_id}&scope=copilot&redirect_uri={redirect}&state={state}",
        auth = GITHUB_AUTH_URL,
        client_id = client_id,
        redirect = urlencoding::encode(redirect_uri),
        state = &state,
    );
    let session = GitHubAuthSession {
        state,
        redirect_uri: redirect_uri.to_owned(),
    };
    (url, session)
}

/// Exchange the authorization code for a GitHub access token.
pub async fn complete_github_auth(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    code: &str,
    _session: GitHubAuthSession,
) -> anyhow::Result<String> {
    let body = [
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code", code),
    ];

    let resp = client
        .post(GITHUB_TOKEN_URL)
        .header("Accept", "application/json")
        .form(&body)
        .send()
        .await
        .context("GitHub token request failed")?
        .error_for_status()
        .context("GitHub token endpoint returned error")?;

    let raw: serde_json::Value = resp.json().await.context("GitHub token parse failed")?;
    raw["access_token"]
        .as_str()
        .context("missing access_token in GitHub response")
        .map(str::to_owned)
}

fn random_state() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
