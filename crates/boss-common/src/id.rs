use uuid::Uuid;

/// Generate a new unique id (e.g. object uid, sandbox id).
pub fn new_uid() -> String {
    Uuid::new_v4().to_string()
}

/// A short, human-friendly id suitable for sandbox ids.
pub fn short_id() -> String {
    Uuid::new_v4().simple().to_string()[..12].to_string()
}
