//! Server-side world time advancement.

use stagcrest_protocol::TimeOfDay;

pub fn advance_world_time(current: f64, dt_secs: f32) -> f64 {
    let mut tod = TimeOfDay::new(current);
    tod.advance(dt_secs as f64);
    tod.seconds()
}
