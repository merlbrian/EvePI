-- Stores in-flight EVE SSO PKCE sessions initiated from the MCP
-- `start_eve_auth` tool. The callback server matches the `state` parameter
-- from the browser redirect to complete the OAuth exchange.
-- Entries older than 10 minutes are considered stale and cleaned up.

CREATE TABLE IF NOT EXISTS pending_auth (
    state         TEXT     NOT NULL PRIMARY KEY,
    pkce_verifier TEXT     NOT NULL,
    redirect_uri  TEXT     NOT NULL,
    account_label TEXT,
    created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
