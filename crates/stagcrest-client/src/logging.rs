//! Tracing subscriber setup.

pub fn init() {
    use tracing_subscriber::{fmt, EnvFilter};

    let _ = fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();
}
