use crate::block_icons::{bake_block_icons, BlockIconCache};
use crate::game::{AppState, ModContext};
use crate::player::{capture_cursor, SelectedBlock};
use bevy::prelude::*;
use bevy::text::EditableTextSystems;
use stagcrest_render::BlockAtlasResource;

pub mod hotbar;
pub mod input;
pub mod screen;
pub mod state;

pub use state::{inventory_open, CreativeInventory, InventoryUiState};

use hotbar::{
    cleanup_hotbar, hotbar_click, hotbar_keyboard, hotbar_scroll, spawn_hotbar,
    update_hotbar_highlight, update_hotbar_visibility,
};
use input::{
    clear_search_on_escape, inventory_click, inventory_cursor_ghost, sync_selected_block,
    toggle_inventory_screen,
};
use screen::{cleanup_inventory_screen, on_search_text_changed, update_scrollbar_thumb};

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedBlock>()
            .init_resource::<InventoryUiState>()
            .add_observer(on_search_text_changed)
            .add_systems(OnEnter(AppState::InGame), setup_inventory)
            .add_systems(
                OnExit(AppState::InGame),
                (
                    cleanup_hotbar,
                    cleanup_inventory_screen,
                    cleanup_inventory_resources,
                ),
            )
            .add_systems(
                Update,
                (
                    toggle_inventory_screen.before(EditableTextSystems),
                    clear_search_on_escape,
                    inventory_click,
                    inventory_cursor_ghost,
                    hotbar_keyboard,
                    hotbar_scroll,
                    hotbar_click,
                    sync_selected_block,
                    update_hotbar_visibility,
                    update_hotbar_highlight,
                    update_scrollbar_thumb,
                    capture_cursor
                        .run_if(in_state(AppState::InGame))
                        .run_if(not(inventory_open)),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn setup_inventory(
    mut commands: Commands,
    theme: Res<crate::ui::UiTheme>,
    mod_ctx: Option<Res<ModContext>>,
    atlas: Option<Res<BlockAtlasResource>>,
    mut images: ResMut<Assets<Image>>,
    icons: Option<Res<BlockIconCache>>,
) {
    let (Some(ctx), Some(atlas)) = (mod_ctx, atlas) else {
        return;
    };

    if icons.is_none() {
        let cache = bake_block_icons(&ctx, &atlas, &mut images);
        commands.insert_resource(cache);
    }

    let placeable = ctx.registry.placeable_blocks();
    if placeable.is_empty() {
        return;
    }

    let inventory = CreativeInventory::from_placeable(placeable);
    let default = inventory
        .selected_block()
        .or_else(|| placeable.first().copied())
        .unwrap();

    commands.insert_resource(inventory);
    commands.insert_resource(SelectedBlock(default));

    spawn_hotbar(&mut commands, &theme);
}

fn cleanup_inventory_resources(mut commands: Commands, mut ui: ResMut<InventoryUiState>) {
    ui.open = false;
    ui.cursor = None;
    commands.remove_resource::<CreativeInventory>();
    commands.remove_resource::<BlockIconCache>();
}
