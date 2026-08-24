//! The Analytics Engine SQL API.

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// The token the API is called with.
///
/// Read from the environment rather than the config file: the config says
/// which dataset to read and belongs in a repository, and this does not.
pub const TOKEN_VARS: [&str; 2] = ["OKIBI_CF_API_TOKEN", "CLOUDFLARE_API_TOKEN"];

/// Where the account id may come from instead of the config file.
///
/// `CLOUDFLARE_ACCOUNT_ID` is the name wrangler already uses, so a workflow
/// that deploys and a workflow that aggregates can read the same variable.
pub const ACCOUNT_VARS: [&str; 2] = ["OKIBI_ACCOUNT_ID", "CLOUDFLARE_ACCOUNT_ID"];

fn first_set(names: &[&str]) -> Option<String> {
    names
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
}

pub fn token_from_env() -> Result<String> {
    first_set(&TOKEN_VARS)
        .ok_or_else(|| anyhow::anyhow!("no API token: set {}", TOKEN_VARS.join(" or ")))
}

/// The account id, from the environment if it is there.
///
/// The environment wins over the config file. Overriding a committed file is
/// the reason the variable exists, and a variable that lost to the file would
/// have to be explained every time someone set it and nothing happened.
pub fn account_from_env(configured: Option<&str>) -> Result<String> {
    if let Some(account) = first_set(&ACCOUNT_VARS) {
        return Ok(account);
    }
    configured
        .filter(|account| !account.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no account id: set {} or put account_id in the config",
                ACCOUNT_VARS.join(" or ")
            )
        })
}

pub struct Client {
    http: reqwest::Client,
    endpoint: String,
    token: String,
}

/// What the SQL API sends back. The rows are whatever the query selected.
#[derive(Debug, Deserialize)]
struct Response<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
    #[serde(default)]
    rows: usize,
}

impl Client {
    pub fn new(account_id: &str, token: String) -> Result<Self> {
        Ok(Client {
            http: reqwest::Client::builder()
                .user_agent(concat!("okibi/", env!("CARGO_PKG_VERSION")))
                .build()?,
            endpoint: format!(
                "https://api.cloudflare.com/client/v4/accounts/{account_id}/analytics_engine/sql"
            ),
            token,
        })
    }

    /// Run one query and read its rows.
    pub async fn query<T: serde::de::DeserializeOwned>(&self, sql: &str) -> Result<Vec<T>> {
        let response = self
            .http
            .post(&self.endpoint)
            .bearer_auth(&self.token)
            .header(reqwest::header::CONTENT_TYPE, "text/plain")
            .body(sql.to_string())
            .send()
            .await
            .context("calling the SQL API")?;

        let status = response.status();
        let body = response.text().await.context("reading the response")?;

        if !status.is_success() {
            bail!("the SQL API answered {status}: {body}");
        }

        let parsed: Response<T> = serde_json::from_str(&body)
            .with_context(|| format!("parsing the response: {}", truncate(&body)))?;

        // `rows` is what the API counted; the vector is what could be read as
        // the shape asked for. A gap means the query and the row type have
        // drifted apart, which otherwise shows up as a digest quietly missing
        // most of its cells.
        if parsed.rows != parsed.data.len() {
            bail!(
                "the SQL API returned {} rows and {} could be read",
                parsed.rows,
                parsed.data.len()
            );
        }
        Ok(parsed.data)
    }
}

fn truncate(body: &str) -> String {
    if body.len() <= 400 {
        return body.to_string();
    }
    format!("{}…", &body[..400])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The environment is process-wide, so these run one at a time behind a
    /// lock rather than as separate tests that would race each other.
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn without_env<T>(work: impl FnOnce() -> T) -> T {
        let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let saved: Vec<(&str, Option<String>)> = ACCOUNT_VARS
            .iter()
            .map(|name| (*name, std::env::var(name).ok()))
            .collect();
        for (name, _) in &saved {
            unsafe { std::env::remove_var(name) };
        }

        let result = work();

        for (name, value) in saved {
            match value {
                Some(value) => unsafe { std::env::set_var(name, value) },
                None => unsafe { std::env::remove_var(name) },
            }
        }
        result
    }

    #[test]
    fn the_config_supplies_the_account_when_nothing_else_does() {
        without_env(|| {
            assert_eq!(account_from_env(Some("from-file")).unwrap(), "from-file");
        });
    }

    /// Overriding a committed file is the reason the variable exists.
    #[test]
    fn the_environment_wins() {
        without_env(|| {
            unsafe { std::env::set_var("OKIBI_ACCOUNT_ID", "from-env") };
            assert_eq!(account_from_env(Some("from-file")).unwrap(), "from-env");
            unsafe { std::env::remove_var("OKIBI_ACCOUNT_ID") };
        });
    }

    #[test]
    fn wranglers_own_variable_is_read_too() {
        without_env(|| {
            unsafe { std::env::set_var("CLOUDFLARE_ACCOUNT_ID", "from-wrangler") };
            assert_eq!(account_from_env(None).unwrap(), "from-wrangler");
            unsafe { std::env::remove_var("CLOUDFLARE_ACCOUNT_ID") };
        });
    }

    #[test]
    fn says_both_places_it_looked() {
        without_env(|| {
            let error = account_from_env(None).unwrap_err().to_string();
            assert!(error.contains("OKIBI_ACCOUNT_ID"), "{error}");
            assert!(error.contains("account_id in the config"), "{error}");
        });
    }

    #[test]
    fn an_empty_value_is_not_a_value() {
        without_env(|| {
            unsafe { std::env::set_var("OKIBI_ACCOUNT_ID", "") };
            assert_eq!(account_from_env(Some("from-file")).unwrap(), "from-file");
            unsafe { std::env::remove_var("OKIBI_ACCOUNT_ID") };
        });
    }
}
