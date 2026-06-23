//! Cross-platform tracing subscriber setup for native and wasm.

/// Install the global tracing subscriber once. Call before `App::new()`.
pub fn init() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use tracing_subscriber::{fmt, EnvFilter};

        let _ = fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .try_init();
    }

    #[cfg(target_arch = "wasm32")]
    {
        tracing_wasm::set_as_global_default();
    }
}
