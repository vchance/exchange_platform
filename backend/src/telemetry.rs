use tracing_subscriber::EnvFilter;

/// Installs the log subscriber. Level comes from `RUST_LOG`, defaulting to `info`.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
