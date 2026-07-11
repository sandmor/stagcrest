//! Graphics settings menu (main menu + pause).

use crate::client_content::{cleanup_screen, spawn_screen_button, ClientContentSettings};
use crate::game::AppState;
use crate::ui::UiTheme;
use bevy::prelude::*;
use stagcrest_render::{GraphicsSettings, QualityTier, ReflectionTier};

/// App state to return to when leaving the graphics screen.
#[derive(Resource, Clone, Debug, Default)]
pub struct GraphicsReturn(pub AppState);

pub struct GraphicsSettingsScreenPlugin;

#[derive(Component)]
struct GraphicsSettingsRoot;

#[derive(Component, Clone, Copy)]
enum GraphicsAction {
    Back,
    CycleTier,
    CycleReflection,
    Save,
}

impl Plugin for GraphicsSettingsScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GraphicsReturn>()
            .add_systems(OnEnter(AppState::GraphicsSettings), spawn_graphics_ui)
            .add_systems(
                OnExit(AppState::GraphicsSettings),
                cleanup_screen::<GraphicsSettingsRoot>,
            )
            .add_systems(
                Update,
                graphics_button_system.run_if(in_state(AppState::GraphicsSettings)),
            );
    }
}

fn spawn_graphics_ui(mut commands: Commands, theme: Res<UiTheme>, settings: Res<GraphicsSettings>) {
    commands
        .spawn((
            GraphicsSettingsRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(theme.screen_bg),
        ))
        .with_children(|root| {
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
                padding: UiRect::all(Val::Px(24.0)),
                min_width: Val::Px(420.0),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            })
            .insert(BackgroundColor(theme.panel_bg))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("Graphics"),
                    theme.text_font(FontSize::Px(30.0)),
                    TextColor(theme.text_primary),
                ));
                panel.spawn((
                    GraphicsSummary,
                    Text::new(summary_text(&settings)),
                    theme.text_font(theme.body_size),
                    TextColor(theme.text_muted),
                ));
                spawn_screen_button(panel, "Quality tier", GraphicsAction::CycleTier, &theme);
                spawn_screen_button(
                    panel,
                    "Water reflections",
                    GraphicsAction::CycleReflection,
                    &theme,
                );
                spawn_screen_button(panel, "Save & back", GraphicsAction::Save, &theme);
                spawn_screen_button(panel, "Back", GraphicsAction::Back, &theme);
            });
        });
}

#[derive(Component)]
struct GraphicsSummary;

fn summary_text(settings: &GraphicsSettings) -> String {
    format!(
        "Tier: {:?}\nReflections: {:?}\nShadows: {} cascades / {:.0}m\nSSAO: {}  Bloom: {}  TAA: {}",
        settings.tier,
        settings.reflection_tier,
        settings.shadow_cascades,
        settings.shadow_distance,
        yes_no(settings.ssao),
        yes_no(settings.bloom),
        yes_no(settings.taa),
    )
}

fn yes_no(v: bool) -> &'static str {
    if v { "on" } else { "off" }
}

fn graphics_button_system(
    mut interaction: Query<(&Interaction, &GraphicsAction), (Changed<Interaction>, With<Button>)>,
    mut settings: ResMut<GraphicsSettings>,
    mut client_settings: ResMut<ClientContentSettings>,
    mut summary: Query<&mut Text, With<GraphicsSummary>>,
    mut next_app: ResMut<NextState<AppState>>,
    return_to: Res<GraphicsReturn>,
) {
    for (interaction, action) in &mut interaction {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            GraphicsAction::CycleTier => {
                settings.tier = match settings.tier {
                    QualityTier::Low => QualityTier::Medium,
                    QualityTier::Medium => QualityTier::High,
                    QualityTier::High => QualityTier::Ultra,
                    QualityTier::Ultra => QualityTier::Low,
                };
                settings.apply_tier_presets();
            }
            GraphicsAction::CycleReflection => {
                settings.reflection_tier = match settings.reflection_tier {
                    ReflectionTier::SkyOnly => ReflectionTier::Ssr,
                    ReflectionTier::Ssr => ReflectionTier::Planar,
                    ReflectionTier::Planar => ReflectionTier::SkyOnly,
                };
            }
            GraphicsAction::Save => {
                client_settings
                    .0
                    .set_graphics(crate::graphics::graphics_section_from_settings(&settings));
                let _ = client_settings.save();
                back(&mut next_app, return_to.0.clone());
            }
            GraphicsAction::Back => back(&mut next_app, return_to.0.clone()),
        }
        if let Ok(mut text) = summary.single_mut() {
            text.0 = summary_text(&settings);
        }
    }
}

fn back(next_app: &mut NextState<AppState>, target: AppState) {
    next_app.set(target);
}
