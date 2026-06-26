/// Build a store key for a resource: `/registry/{type}/{namespace}/{name}` for
/// namespaced resources, `/registry/{type}/{name}` for cluster-scoped.
pub fn build_key(resource: &str, namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(ns) if !ns.is_empty() => format!("/registry/{resource}/{ns}/{name}"),
        _ => format!("/registry/{resource}/{name}"),
    }
}

/// Build a prefix for listing/watching: `/registry/{type}/` or
/// `/registry/{type}/{ns}/`.
pub fn build_prefix(resource: &str, namespace: Option<&str>) -> String {
    match namespace {
        Some(ns) if !ns.is_empty() => format!("/registry/{resource}/{ns}/"),
        _ => format!("/registry/{resource}/"),
    }
}
