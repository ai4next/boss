use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Initialize tracing with a default `info` filter overridable via `RUST_LOG`.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .try_init();
}
