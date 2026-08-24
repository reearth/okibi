//! The Analytics Engine SQL API.

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// The token the API is called with.
///
/// Read from the environment rather than the config file: the config says
/// which dataset to read and belongs in a repository, and this does not.
pub const TOKEN_VARS: [&str; 2] = ["OKIBI_CF_API_TOKEN", "CLOUDFLARE_API_TOKEN"];

pub fn token_from_env() -> Result<String> {
    for name in TOKEN_VARS {
        if let Ok(token) = std::env::var(name) {
            if !token.is_empty() {
                return Ok(token);
            }
        }
    }
    bail!("no API token: set {}", TOKEN_VARS.join(" or "))
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
