//! Watch: convert a store watch stream into a newline-delimited JSON
//! response (`{"type":"ADDED","object":{...}}\n` per event).

use std::collections::BTreeMap;

use axum::body::Body;
use axum::http::HeaderValue;
use axum::response::Response;
use boss_api::{EventType, ResourceVersion, WatchEvent as ApiWatchEvent};
use bytes::Bytes;
use tokio_stream::StreamExt;

/// Read `metadata.resourceVersion` (as u64) from a JSON value.
fn extract_rv(value: &serde_json::Value) -> ResourceVersion {
    value
        .get("metadata")
        .and_then(|m| m.get("resourceVersion"))
        .and_then(|v| v.as_u64())
        .map(ResourceVersion)
        .unwrap_or(ResourceVersion(0))
}

/// Build a streaming `Response` from a store `WatchStream`.
pub fn watch_response(stream: boss_store::WatchStream) -> Response {
    let mapped = stream.map(|ev| {
        let (kind, object) = match ev {
            boss_store::WatchEvent::Added(_, o) => (EventType::Added, o),
            boss_store::WatchEvent::Modified(_, o) => (EventType::Modified, o),
            boss_store::WatchEvent::Deleted(_, o) => (EventType::Deleted, o),
        };
        let rv = extract_rv(&object);
        let we = ApiWatchEvent {
            kind,
            resource_version: rv,
            object,
        };
        let mut line = serde_json::to_vec(&we).unwrap_or_else(|_| b"{}".to_vec());
        line.push(b'\n');
        Ok::<Bytes, std::io::Error>(Bytes::from(line))
    });
    let mut resp = Response::new(Body::from_stream(mapped));
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    resp
}

/// Parse watch query params: returns `(watch?, start_rv)`.
pub fn parse_watch_params(params: &BTreeMap<String, String>) -> (bool, ResourceVersion) {
    let watch = params
        .get("watch")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let rv = params
        .get("resourceVersion")
        .and_then(|s| s.parse::<u64>().ok())
        .map(ResourceVersion)
        .unwrap_or(ResourceVersion(0));
    (watch, rv)
}
