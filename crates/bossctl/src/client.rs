use anyhow::{Context, Result, anyhow};
use reqwest::{Client, Response};

/// HTTP client for the boss apiserver.
#[derive(Clone)]
pub struct ApiClient {
    base: String,
    http: Client,
}

impl ApiClient {
    pub fn new(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            http: Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    pub async fn post(&self, path: &str, body: &serde_json::Value) -> Result<Response> {
        self.http
            .post(self.url(path))
            .json(body)
            .send()
            .await
            .context("POST request failed")
    }

    pub async fn get(&self, path: &str) -> Result<Response> {
        self.http
            .get(self.url(path))
            .send()
            .await
            .context("GET request failed")
    }

    pub async fn put(&self, path: &str, body: &serde_json::Value) -> Result<Response> {
        self.http
            .put(self.url(path))
            .json(body)
            .send()
            .await
            .context("PUT request failed")
    }

    pub async fn delete(&self, path: &str) -> Result<Response> {
        self.http
            .delete(self.url(path))
            .send()
            .await
            .context("DELETE request failed")
    }

    /// Fail if the response status is not success, returning the apiserver
    /// message body when present.
    pub async fn ensure_success(resp: Response) -> Result<Response> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let body = resp.text().await.unwrap_or_default();
        Err(anyhow!("apiserver returned {status}: {body}"))
    }
}
