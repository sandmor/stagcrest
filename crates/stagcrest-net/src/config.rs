/// TCP/socket tuning for game connections.
#[derive(Debug, Clone)]
pub struct NetConfig {
    pub tcp_nodelay: bool,
    pub send_buffer_bytes: usize,
    pub recv_buffer_bytes: usize,
    pub max_priority_queue: usize,
    pub max_bulk_queue: usize,
    /// Artificial latency for dev testing (milliseconds).
    pub sim_latency_ms: u64,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self {
            tcp_nodelay: true,
            send_buffer_bytes: 256 * 1024,
            recv_buffer_bytes: 256 * 1024,
            max_priority_queue: 64,
            max_bulk_queue: 32,
            sim_latency_ms: 0,
        }
    }
}
