use bevy::prelude::*;

pub use crate::inventory::InventoryPlugin;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InventoryPlugin);
    }
}
