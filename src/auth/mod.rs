//! Authentication module.
//!
//! Owns all token I/O. Other modules receive ready-to-use Bearer strings;
//! they never read or write tokens directly.
//!
//! - EVE SSO OAuth2 (PKCE) for character-scoped ESI access
//! - GitHub OAuth (scope: `copilot`) for Copilot API access
//! - Token storage: OS keychain via `keyring`, falling back to an
//!   `age`-encrypted file at `~/.config/evepi/tokens.age`

pub mod enroll;
pub mod eve_sso;
mod github_oauth;
pub mod store;

pub use enroll::{run_enroll, run_github_auth};
pub use eve_sso::{EveAuthSession, begin_eve_auth, complete_eve_auth, refresh_eve_token};
pub use github_oauth::{begin_github_auth, complete_github_auth};
pub use store::{TokenStore, TokenStoreError};
