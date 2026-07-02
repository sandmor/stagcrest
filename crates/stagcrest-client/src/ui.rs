use bevy::prelude::*;
use bevy::text::FontSource;

pub use crate::inventory::InventoryPlugin;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiTheme>()
            .add_plugins(InventoryPlugin)
            .add_systems(Update, crate::client_content::update_scrollbar_thumb);
    }
}

/// Shared UI colors, fonts, and text sizes for client screens.
#[derive(Resource, Clone)]
pub struct UiTheme {
    pub font: FontSource,
    pub title_size: FontSize,
    pub subtitle_size: FontSize,
    pub body_size: FontSize,
    pub caption_size: FontSize,
    pub hint_size: FontSize,
    pub text_primary: Color,
    pub text_muted: Color,
    pub text_hint: Color,
    pub screen_bg: Color,
    pub panel_bg: Color,
    pub overlay_bg: Color,
    pub button_bg: Color,
    pub button_hover: Color,
    pub slot_bg: Color,
    pub slot_border: Color,
    pub slot_selected: Color,
    pub divider: Color,
    pub search_bg: Color,
    pub accent: Color,
    pub catalog_cell_bg: Color,
    pub error_text: Color,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            font: FontSource::SansSerif,
            title_size: FontSize::Px(64.0),
            subtitle_size: FontSize::Px(18.0),
            body_size: FontSize::Px(22.0),
            caption_size: FontSize::Px(14.0),
            hint_size: FontSize::Px(12.0),
            text_primary: Color::WHITE,
            text_muted: Color::srgb(0.6, 0.6, 0.65),
            text_hint: Color::srgb(0.65, 0.65, 0.7),
            screen_bg: Color::srgb(0.08, 0.09, 0.12),
            panel_bg: Color::srgb(0.14, 0.15, 0.19),
            overlay_bg: Color::srgba(0.0, 0.0, 0.0, 0.55),
            button_bg: Color::srgb(0.2, 0.22, 0.28),
            button_hover: Color::srgb(0.28, 0.32, 0.4),
            slot_bg: Color::srgba(0.18, 0.19, 0.24, 0.95),
            slot_border: Color::srgba(0.35, 0.35, 0.4, 0.9),
            slot_selected: Color::WHITE,
            divider: Color::srgba(0.4, 0.4, 0.46, 0.6),
            search_bg: Color::srgb(0.1, 0.11, 0.14),
            accent: Color::srgb(0.9, 0.85, 0.7),
            catalog_cell_bg: Color::srgb(0.22, 0.23, 0.28),
            error_text: Color::srgb(1.0, 0.4, 0.4),
        }
    }
}

impl UiTheme {
    pub fn text_font(&self, size: FontSize) -> TextFont {
        TextFont {
            font: self.font.clone(),
            font_size: size,
            ..default()
        }
    }
}
