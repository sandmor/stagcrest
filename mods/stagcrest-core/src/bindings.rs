wit_bindgen::generate!({
    world: "plugin",
    path: "../../wit",
});

use crate::{commands, guest, register_content};

struct CoreMod;

impl Guest for CoreMod {
    fn register() -> i32 {
        let mut reg = guest::HostRegistrar;
        register_content(&mut reg);
        commands::register_commands();
        0
    }

    fn handle_command() -> i32 {
        commands::handle_command()
    }

    fn on_place(
        _pos: stagcrest::plugin::types::BlockPos,
        _id: stagcrest::plugin::types::BlockId,
        _state: stagcrest::plugin::types::BlockState,
    ) -> stagcrest::plugin::types::BehaviorResult {
        stagcrest::plugin::types::BehaviorResult::Ok
    }

    fn on_break(
        _pos: stagcrest::plugin::types::BlockPos,
        _id: stagcrest::plugin::types::BlockId,
        _state: stagcrest::plugin::types::BlockState,
    ) -> stagcrest::plugin::types::BehaviorResult {
        stagcrest::plugin::types::BehaviorResult::Ok
    }

    fn on_use(
        _pos: stagcrest::plugin::types::BlockPos,
        _id: stagcrest::plugin::types::BlockId,
        _state: stagcrest::plugin::types::BlockState,
    ) -> stagcrest::plugin::types::BehaviorResult {
        stagcrest::plugin::types::BehaviorResult::Ok
    }

    fn on_neighbor_changed(
        _pos: stagcrest::plugin::types::BlockPos,
        _id: stagcrest::plugin::types::BlockId,
        _state: stagcrest::plugin::types::BlockState,
        _neighbor: stagcrest::plugin::types::BlockPos,
    ) -> stagcrest::plugin::types::BehaviorResult {
        stagcrest::plugin::types::BehaviorResult::Ok
    }

    fn on_scheduled_tick(
        _pos: stagcrest::plugin::types::BlockPos,
        _id: stagcrest::plugin::types::BlockId,
        _state: stagcrest::plugin::types::BlockState,
    ) -> stagcrest::plugin::types::BehaviorResult {
        stagcrest::plugin::types::BehaviorResult::Ok
    }

    fn on_random_tick(
        _pos: stagcrest::plugin::types::BlockPos,
        _id: stagcrest::plugin::types::BlockId,
        _state: stagcrest::plugin::types::BlockState,
    ) -> stagcrest::plugin::types::BehaviorResult {
        stagcrest::plugin::types::BehaviorResult::Ok
    }

    fn state_for_place(
        _id: stagcrest::plugin::types::BlockId,
        _face_normal: (i32, i32, i32),
    ) -> Option<stagcrest::plugin::types::BlockState> {
        None
    }

    fn dynamic_light(
        _id: stagcrest::plugin::types::BlockId,
        _state: stagcrest::plugin::types::BlockState,
    ) -> u8 {
        0
    }
}

export!(CoreMod);
