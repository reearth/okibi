//! Which services to aggregate, and where from.

use std::path::Path;

use anyhow::{Context, Result, bail};
use okibi_core::DigestQuery;
use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: &str = "okibi-digest-config/1";

/// The whole of what `okibi digest` needs to know beyond credentials.
///
/// One process aggregates every service, because under the WAE binding there
/// is one dataset and the index is the service: a per-service implementation
/// would be the same query run several times.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub config: String,
    /// The Cloudflare account the dataset belongs to.
    ///
    /// Optional here because this file is committed and an account id is an
    /// identifier nobody needs published. The environment supplies it in CI;
    /// see [`crate::wae::account_from_env`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// What the queries are shaped by, which is okibi's own document.
    #[serde(flatten)]
    pub query: DigestQuery,
}

impl Default for Config {
    /// Every service in the dataset, which is what the dataset is.
    ///
    /// A roster of services would be a second list of who exists, kept beside
    /// the one that already exists — and the one that already exists is the
    /// dataset: a service that writes events appears, one that does not, does
    /// not. Nothing has to be added to a file for a new service to be
    /// aggregated.
    fn default() -> Self {
        Config {
            config: CONFIG_VERSION.to_string(),
            account_id: None,
            query: DigestQuery::default(),
        }
    }
}

impl Config {
    /// The config, or the defaults if there is no file.
    ///
    /// Absent is a valid answer: the defaults aggregate the whole dataset, and
    /// the account comes from the environment, so an installation that wants
    /// nothing unusual configures nothing.
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Config::default());
        }
        Config::load(path)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let config: Config =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;

        if config.config != CONFIG_VERSION {
            bail!(
                "{} says {:?}, and this reads {CONFIG_VERSION}",
                path.display(),
                config.config
            );
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_in_what_was_left_out() {
        let config: Config =
            serde_json::from_str(r#"{"config": "okibi-digest-config/1", "account_id": "abc"}"#)
                .unwrap();

        assert_eq!(config.query.dataset, "tile_demand_1");
        assert_eq!(config.account_id.as_deref(), Some("abc"));
        assert_eq!(config.query.top_n, 20);
        assert!(config.query.services.is_empty());
    }

    /// An installation that wants nothing unusual should configure nothing.
    #[test]
    fn no_file_is_a_valid_configuration() {
        let missing = std::env::temp_dir().join("okibi-no-such-config.json");
        let _ = std::fs::remove_file(&missing);

        let config = Config::load_or_default(&missing).unwrap();
        assert_eq!(config.query.dataset, "tile_demand_1");
        assert!(
            config.query.services.is_empty(),
            "which reads every service in the dataset"
        );
    }

    #[test]
    fn refuses_a_version_it_does_not_read() {
        let dir = std::env::temp_dir().join("okibi-config-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("services.json");
        std::fs::write(
            &path,
            r#"{"config": "okibi-digest-config/2", "account_id": "abc"}"#,
        )
        .unwrap();

        let error = Config::load(&path).unwrap_err().to_string();
        assert!(error.contains("okibi-digest-config/1"), "{error}");
    }
}
