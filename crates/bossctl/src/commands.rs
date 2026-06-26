use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use tokio_stream::StreamExt;

use crate::client::ApiClient;

/// Whether a resource kind is cluster-scoped (vs namespaced).
fn is_cluster_scoped(kind: &str) -> bool {
    matches!(kind, "Node" | "Namespace")
}

/// Plural collection segment for a kind.
fn plural(kind: &str) -> &str {
    match kind {
        "Pod" => "pods",
        "Node" => "nodes",
        "Deployment" => "deployments",
        "ReplicaSet" => "replicasets",
        "Lease" => "leases",
        other => other.to_lowercase().leak(),
    }
}

fn collection_path(kind: &str, namespace: &str) -> String {
    if is_cluster_scoped(kind) {
        format!("/api/v1/{}", plural(kind))
    } else {
        format!("/api/v1/namespaces/{namespace}/{}", plural(kind))
    }
}

fn item_path(kind: &str, namespace: &str, name: &str) -> String {
    format!("{}/{}", collection_path(kind, namespace), name)
}

/// Read a YAML or JSON manifest into a JSON value.
fn read_manifest(path: &Path) -> Result<Value> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    if path.extension().and_then(|e| e.to_str()) == Some("json") {
        Ok(serde_json::from_str(&raw)?)
    } else {
        // serde_yaml can parse into serde_json::Value
        Ok(serde_yaml::from_str(&raw)?)
    }
}

pub async fn apply(client: &ApiClient, file: &Path) -> Result<()> {
    let mut doc = read_manifest(file)?;
    let kind = doc
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("manifest missing 'kind'"))?
        .to_string();
    let name = doc
        .pointer("/metadata/name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("manifest missing 'metadata.name'"))?
        .to_string();
    let namespace = doc
        .pointer("/metadata/namespace")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();

    let create_path = collection_path(&kind, &namespace);
    let item = item_path(&kind, &namespace, &name);

    let resp = client.post(&create_path, &doc).await?;
    if resp.status().as_u16() == 409 {
        // Already exists: replace via PUT, carrying resourceVersion + uid.
        let existing: Value = ApiClient::ensure_success(client.get(&item).await?)
            .await?
            .json()
            .await?;
        if let (Some(rv), Some(meta)) = (
            existing.pointer("/metadata/resourceVersion").cloned(),
            doc.pointer_mut("/metadata").and_then(|m| m.as_object_mut()),
        ) {
            meta.insert("resourceVersion".into(), rv);
            if let Some(uid) = existing.pointer("/metadata/uid").cloned() {
                meta.insert("uid".into(), uid);
            }
        }
        let resp = client.put(&item, &doc).await?;
        let v: Value = ApiClient::ensure_success(resp).await?.json().await?;
        print_pretty(&v);
        println!("replaced");
        return Ok(());
    }
    let v: Value = ApiClient::ensure_success(resp).await?.json().await?;
    print_pretty(&v);
    println!("created");
    Ok(())
}

pub async fn get(
    client: &ApiClient,
    resource: &str,
    name: Option<&str>,
    namespace: &str,
) -> Result<()> {
    let kind = singular_to_kind(resource)?;
    match name {
        Some(n) => {
            let resp =
                ApiClient::ensure_success(client.get(&item_path(&kind, namespace, n)).await?)
                    .await?;
            let v: Value = resp.json().await?;
            print_pretty(&v);
        }
        None => {
            let resp =
                ApiClient::ensure_success(client.get(&collection_path(&kind, namespace)).await?)
                    .await?;
            let v: Value = resp.json().await?;
            print_list(&v);
        }
    }
    Ok(())
}

pub async fn delete(client: &ApiClient, resource: &str, name: &str, namespace: &str) -> Result<()> {
    let kind = singular_to_kind(resource)?;
    let resp =
        ApiClient::ensure_success(client.delete(&item_path(&kind, namespace, name)).await?).await?;
    let status = resp.status();
    println!("deleted ({status})");
    Ok(())
}

pub async fn watch(client: &ApiClient, resource: &str, namespace: &str) -> Result<()> {
    let kind = singular_to_kind(resource)?;
    let path = format!(
        "{}?watch=true&resourceVersion=0",
        collection_path(&kind, namespace)
    );
    let resp = client.get(&path).await?;
    if !resp.status().is_success() {
        bail!("watch failed: {}", resp.status());
    }
    let mut stream = resp.bytes_stream();
    let mut buf = Vec::<u8>::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("stream error")?;
        buf.extend_from_slice(&chunk);
        while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line).trim().to_string();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(&line) {
                Ok(ev) => print_event(&ev),
                Err(_) => tracing::warn!(line, "unparseable watch line"),
            }
        }
    }
    Ok(())
}

fn print_event(ev: &Value) {
    let ty = ev.get("type").and_then(|v| v.as_str()).unwrap_or("?");
    let kind = ev
        .pointer("/object/kind")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let name = ev
        .pointer("/object/metadata/name")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let ns = ev
        .pointer("/object/metadata/namespace")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let phase = ev
        .pointer("/object/status/phase")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    println!("{ty:>8} {kind}/{ns}/{name} {phase}");
}

fn print_pretty(v: &Value) {
    if let Ok(s) = serde_json::to_string_pretty(v) {
        println!("{s}");
    }
}

fn print_list(v: &Value) {
    let items = v.get("items").and_then(|i| i.as_array());
    match items {
        Some(items) if !items.is_empty() => {
            let kind = v.get("kind").and_then(|kind| kind.as_str()).unwrap_or("");
            if kind == "PodList" {
                println!("NAME\tPHASE\tNODE\tCLASS\tPROVIDER\tSANDBOX-ID\tREASON");
            } else if kind == "NodeList" {
                println!("NAME\tREADY\tPROVIDERS");
            } else if kind == "DeploymentList" || kind == "ReplicaSetList" {
                println!("NAME\tREPLICAS\tREADY\tAVAILABLE\tREASON");
            }
            for it in items {
                let name = it
                    .pointer("/metadata/name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let ns = it
                    .pointer("/metadata/namespace")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let phase = it
                    .pointer("/status/phase")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let node = it
                    .pointer("/spec/nodeName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<none>");
                let id = if ns.is_empty() {
                    name.to_string()
                } else {
                    format!("{ns}/{name}")
                };
                if kind == "PodList" {
                    let class = it
                        .pointer("/status/sandboxClass")
                        .and_then(|v| v.as_str())
                        .or_else(|| it.pointer("/spec/sandboxClass").and_then(|v| v.as_str()))
                        .or_else(|| it.pointer("/spec/runtimeClass").and_then(|v| v.as_str()))
                        .unwrap_or("process");
                    let provider = it
                        .pointer("/status/provider")
                        .and_then(|v| v.as_str())
                        .or_else(|| {
                            it.pointer("/metadata/annotations/boss.io~1selected-provider")
                                .and_then(|v| v.as_str())
                        })
                        .unwrap_or("<none>");
                    let sandbox_id = it
                        .pointer("/status/sandboxID")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<none>");
                    let reason = it
                        .pointer("/status/reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("-");
                    println!("{id}\t{phase}\t{node}\t{class}\t{provider}\t{sandbox_id}\t{reason}");
                } else if kind == "NodeList" {
                    let ready = node_ready(it);
                    let providers = node_providers(it);
                    println!("{name}\t{ready}\t{providers}");
                } else if kind == "DeploymentList" || kind == "ReplicaSetList" {
                    let replicas = it
                        .pointer("/status/replicas")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let ready = it
                        .pointer("/status/readyReplicas")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let available = it
                        .pointer("/status/availableReplicas")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let reason = top_condition_reason(it).unwrap_or("-");
                    println!("{id}\t{replicas}\t{ready}\t{available}\t{reason}");
                } else {
                    println!("{id}\t{phase}\t{node}");
                }
            }
        }
        _ => println!("No resources found."),
    }
}

fn top_condition_reason(item: &Value) -> Option<&str> {
    item.pointer("/status/conditions")
        .and_then(|v| v.as_array())
        .and_then(|conditions| {
            conditions
                .iter()
                .find(|condition| {
                    condition.get("type").and_then(|v| v.as_str()) == Some("Degraded")
                        && condition.get("status").and_then(|v| v.as_str()) == Some("True")
                })
                .or_else(|| conditions.first())
        })
        .and_then(|condition| condition.get("reason"))
        .and_then(|v| v.as_str())
}

fn node_ready(node: &Value) -> &'static str {
    node.pointer("/status/conditions")
        .and_then(|v| v.as_array())
        .and_then(|conditions| {
            conditions.iter().find(|condition| {
                condition
                    .get("type")
                    .and_then(|v| v.as_str())
                    .is_some_and(|kind| kind == "Ready")
            })
        })
        .and_then(|condition| condition.get("status"))
        .and_then(|v| v.as_str())
        .filter(|status| *status == "True")
        .map(|_| "True")
        .unwrap_or("False")
}

fn node_providers(node: &Value) -> String {
    let Some(providers) = node
        .pointer("/status/runtimeCapabilities/providers")
        .and_then(|v| v.as_array())
    else {
        return "<none>".to_string();
    };
    providers
        .iter()
        .map(|provider| {
            let name = provider.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let healthy = provider
                .get("healthy")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let classes = provider
                .get("classes")
                .and_then(|v| v.as_array())
                .map(|classes| {
                    classes
                        .iter()
                        .filter_map(|class| class.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            format!("{name}({classes},healthy={healthy})")
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn singular_to_kind(resource: &str) -> Result<String> {
    let k = match resource.to_lowercase().as_str() {
        "pod" | "pods" => "Pod",
        "node" | "nodes" => "Node",
        "deployment" | "deployments" => "Deployment",
        "replicaset" | "replicasets" => "ReplicaSet",
        "lease" | "leases" => "Lease",
        other => bail!("unknown resource kind: {other}"),
    };
    Ok(k.to_string())
}
