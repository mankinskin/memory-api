use std::env;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

const DEFAULT_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider.config.missing_api_key: missing environment variable '{env_var}'")]
    MissingApiKey { env_var: String },
    #[error("provider.config.invalid_base_url: {0}")]
    InvalidBaseUrl(url::ParseError),
    #[error("provider.http.invalid_header")]
    InvalidHeader,
    #[error("provider.http.transport: {0}")]
    Transport(reqwest::Error),
    #[error("provider.http.unexpected_status: status={status}, body={body}")]
    UnexpectedStatus { status: u16, body: String },
}

#[derive(Debug, Clone)]
pub struct CopilotApiConfig {
    pub base_url: Url,
    pub api_key_env: String,
    pub endpoint_path: String,
    pub timeout: Duration,
}

impl CopilotApiConfig {
    pub fn from_env() -> Result<Self, ProviderError> {
        let base_url = env::var("COPILOT_API_BASE_URL")
            .unwrap_or_else(|_| "https://api.githubcopilot.com".to_string());
        let endpoint_path =
            env::var("COPILOT_API_SUBAGENT_ENDPOINT").unwrap_or_else(|_| "/v1/subagents/start".to_string());
        let timeout_secs = env::var("COPILOT_API_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_SECS);

        Ok(Self {
            base_url: Url::parse(&base_url).map_err(ProviderError::InvalidBaseUrl)?,
            api_key_env: "COPILOT_API_KEY".to_string(),
            endpoint_path,
            timeout: Duration::from_secs(timeout_secs),
        })
    }

    pub fn resolve_endpoint(&self) -> Result<Url, ProviderError> {
        self.base_url
            .join(self.endpoint_path.trim_start_matches('/'))
            .map_err(ProviderError::InvalidBaseUrl)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StartSubagentRequest {
    pub ticket_id: String,
    pub assignment_id: String,
    pub prompt: String,
    pub branch: String,
    pub worktree_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartSubagentResponse {
    pub run_id: String,
    pub status: String,
}

pub trait SubagentProvider {
    fn start_subagent(
        &self,
        request: &StartSubagentRequest,
    ) -> Result<StartSubagentResponse, ProviderError>;
}

pub struct CopilotApiClient {
    http: Client,
    config: CopilotApiConfig,
}

impl CopilotApiClient {
    pub fn new(config: CopilotApiConfig) -> Result<Self, ProviderError> {
        let http = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(ProviderError::Transport)?;
        Ok(Self { http, config })
    }

    pub fn redacted_api_key_for_logs(&self) -> Result<String, ProviderError> {
        let value = env::var(&self.config.api_key_env).map_err(|_| ProviderError::MissingApiKey {
            env_var: self.config.api_key_env.clone(),
        })?;
        Ok(redact_secret(&value))
    }
}

impl SubagentProvider for CopilotApiClient {
    fn start_subagent(
        &self,
        request: &StartSubagentRequest,
    ) -> Result<StartSubagentResponse, ProviderError> {
        let endpoint = self.config.resolve_endpoint()?;
        let api_key = env::var(&self.config.api_key_env).map_err(|_| ProviderError::MissingApiKey {
            env_var: self.config.api_key_env.clone(),
        })?;

        let mut headers = HeaderMap::new();
        let auth = format!("Bearer {api_key}");
        let auth_value = HeaderValue::from_str(&auth).map_err(|_| ProviderError::InvalidHeader)?;
        headers.insert(AUTHORIZATION, auth_value);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let response = self
            .http
            .post(endpoint)
            .headers(headers)
            .json(request)
            .send()
            .map_err(ProviderError::Transport)?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(ProviderError::UnexpectedStatus {
                status: status.as_u16(),
                body,
            });
        }

        response.json().map_err(ProviderError::Transport)
    }
}

pub fn redact_secret(secret: &str) -> String {
    if secret.len() <= 6 {
        return "***".to_string();
    }
    let head = &secret[..3];
    let tail = &secret[secret.len() - 3..];
    format!("{head}***{tail}")
}

#[cfg(test)]
mod tests {
    use super::redact_secret;

    #[test]
    fn redacts_short_secrets() {
        assert_eq!(redact_secret("abc"), "***");
    }

    #[test]
    fn redacts_longer_secrets() {
        assert_eq!(redact_secret("abcdefghijk"), "abc***ijk");
    }
}
