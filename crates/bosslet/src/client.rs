use anyhow::{Context, Result, anyhow};
use boss_api::{Node, Pod, ResourceVersion};
use reqwest::{Client, Response, StatusCode};

/// HTTP client for the boss apiserver, used by the bosslet.
#[derive(Clone)]
pub struct ApiServerClient {
    base: String,
    http: Client,
}

impl ApiServerClient {
    pub fn new(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            http: Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    // ---- Pods ----

    pub async fn list_pods(&self) -> Result<Vec<Pod>> {
        let resp: Response = self.http.get(self.url("/api/v1/pods")).send().await?;
        let list: boss_api::ObjectList<PodSpec> = decode(resp).await?;
        Ok(list.items)
    }

    pub async fn get_pod(&self, namespace: &str, name: &str) -> Result<Pod> {
        let resp = self
            .http
            .get(self.url(&format!("/api/v1/namespaces/{namespace}/pods/{name}")))
            .send()
            .await?;
        decode(resp).await
    }

    pub async fn update_pod_status(&self, namespace: &str, name: &str, pod: &Pod) -> Result<Pod> {
        let resp = self
            .http
            .put(self.url(&format!(
                "/api/v1/namespaces/{namespace}/pods/{name}/status"
            )))
            .json(pod)
            .send()
            .await?;
        decode(resp).await
    }

    /// Start a watch over all pods from `rv`. The caller reads the newline-
    /// delimited JSON stream from the response body.
    pub async fn watch_pods_raw(&self, rv: ResourceVersion) -> Result<Response> {
        let resp = self
            .http
            .get(self.url(&format!("/api/v1/pods?watch=true&resourceVersion={}", rv.0)))
            .send()
            .await
            .context("watch request failed")?;
        if !resp.status().is_success() {
            return Err(anyhow!("watch failed: {}", resp.status()));
        }
        Ok(resp)
    }

    // ---- Nodes ----

    pub async fn get_node(&self, name: &str) -> Result<Option<Node>> {
        let resp = self
            .http
            .get(self.url(&format!("/api/v1/nodes/{name}")))
            .send()
            .await?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let node: Node = decode(resp).await?;
        Ok(Some(node))
    }

    pub async fn create_node(&self, node: &Node) -> Result<Node> {
        let resp = self
            .http
            .post(self.url("/api/v1/nodes"))
            .json(node)
            .send()
            .await?;
        decode(resp).await
    }

    pub async fn update_node(&self, name: &str, node: &Node) -> Result<Node> {
        let resp = self
            .http
            .put(self.url(&format!("/api/v1/nodes/{name}")))
            .json(node)
            .send()
            .await?;
        decode(resp).await
    }

    pub async fn update_node_status(&self, name: &str, node: &Node) -> Result<Node> {
        let resp = self
            .http
            .put(self.url(&format!("/api/v1/nodes/{name}/status")))
            .json(node)
            .send()
            .await?;
        decode(resp).await
    }
}

async fn decode<T: serde::de::DeserializeOwned>(resp: Response) -> Result<T> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("apiserver {status}: {body}"));
    }
    let v = resp.json::<T>().await.context("decode response")?;
    Ok(v)
}

// Re-export so the bosslet can reference the spec type.
use boss_api::PodSpec;
