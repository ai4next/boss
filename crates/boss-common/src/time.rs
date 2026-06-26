use chrono::Utc;

/// RFC3339 UTC timestamp.
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}
