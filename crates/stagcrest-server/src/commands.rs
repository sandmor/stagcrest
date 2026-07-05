use stagcrest_mod_server::CommandHost;
use stagcrest_net::{ChatKind, ChatLine};

use crate::client_session::{ClientId, ClientRegistry};
use crate::session::WorldSession;

/// Adapter exposing live server state to mod command callbacks via the
/// [`CommandHost`] trait. Borrows the world session (for the day/night clock)
/// and the client registry (for targeted chat replies) for the duration of a
/// single command dispatch.
pub struct CommandHostImpl<'a> {
    pub session: &'a mut WorldSession,
    pub clients: &'a mut ClientRegistry,
}

impl<'a> CommandHost for CommandHostImpl<'a> {
    fn set_world_time(&mut self, time: f64) {
        let wrapped = stagcrest_protocol::TimeOfDay::new(time).seconds();
        self.session.meta.world_time = wrapped;
        self.clients.broadcast_world_time(wrapped);
    }

    fn world_time(&self) -> f64 {
        self.session.meta.world_time
    }

    fn send_chat_to(&mut self, client_id: u64, text: String) {
        self.clients.send_chat_to(
            ClientId(client_id),
            ChatLine {
                kind: ChatKind::System,
                text,
            },
        );
    }
}
