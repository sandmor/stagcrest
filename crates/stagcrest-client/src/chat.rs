use std::collections::VecDeque;
use std::time::{Duration, Instant};

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::text::{EditableText, EditableTextSystems, TextCursorStyle};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use stagcrest_net::ChatLine;

use crate::game::{AppState, GameplayState};
use crate::game_session::GameCamera;
use crate::inventory::input::editable_text_has_focus;
use crate::net_client::GameNetClient;
use crate::player::{release_cursor, FlyCamera};
use crate::ui::UiTheme;

const MAX_MESSAGES: usize = 100;
const FULL_VISIBLE: Duration = Duration::from_secs(10);
const FADE_DURATION: Duration = Duration::from_secs(5);

pub struct ChatPlugin;

#[derive(Resource, Default)]
pub struct ChatUiState {
    pub input_open: bool,
    messages: VecDeque<ChatLineLocal>,
    pub dirty: bool,
}

struct ChatLineLocal {
    line: ChatLine,
    received_at: Instant,
}

#[derive(Component)]
pub struct ChatRoot;

#[derive(Component)]
struct ChatLog;

#[derive(Component)]
struct ChatLineWidget;

#[derive(Component)]
struct ChatInputRow;

#[derive(Component)]
pub struct ChatInputField;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatUiState>().add_systems(
            Update,
            (
                toggle_chat_input.before(EditableTextSystems),
                submit_chat.after(EditableTextSystems),
                close_chat_input,
                update_chat_display,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

pub fn chat_input_open(chat: Res<ChatUiState>) -> bool {
    chat.input_open
}

pub fn push_chat_line(chat: &mut ChatUiState, line: ChatLine) {
    chat.messages.push_back(ChatLineLocal {
        line,
        received_at: Instant::now(),
    });
    while chat.messages.len() > MAX_MESSAGES {
        chat.messages.pop_front();
    }
    chat.dirty = true;
}

pub fn cleanup_chat(commands: &mut Commands, roots: &Query<Entity, With<ChatRoot>>) {
    for entity in roots {
        commands.entity(entity).despawn();
    }
    commands.insert_resource(ChatUiState::default());
}

pub fn spawn_chat_overlay(commands: &mut Commands, theme: &UiTheme) {
    commands
        .spawn((
            crate::game_session::GameSessionEntity,
            ChatRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(4.0),
                bottom: Val::Px(90.0),
                width: Val::Percent(40.0),
                max_height: Val::Percent(40.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                ChatLog,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(1.0),
                    ..default()
                },
            ));
        });

    commands
        .spawn((
            crate::game_session::GameSessionEntity,
            ChatInputRow,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                padding: UiRect::all(Val::Px(4.0)),
                ..default()
            },
            Visibility::Hidden,
        ))
        .insert(BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)))
        .with_children(|parent| {
            let mut edit = EditableText::new("");
            edit.allow_newlines = false;
            parent.spawn((
                ChatInputField,
                edit,
                TextLayout::no_wrap(),
                theme.text_font(theme.body_size),
                TextCursorStyle::default(),
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::all(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(theme.search_bg.with_alpha(0.85)),
            ));
        });
}

fn line_alpha(received_at: Instant) -> f32 {
    let elapsed = received_at.elapsed();
    if elapsed <= FULL_VISIBLE {
        return 1.0;
    }
    let fade_elapsed = elapsed - FULL_VISIBLE;
    if fade_elapsed >= FADE_DURATION {
        return 0.0;
    }
    1.0 - fade_elapsed.as_secs_f32() / FADE_DURATION.as_secs_f32()
}

fn format_line(line: &ChatLine) -> String {
    match &line.kind {
        stagcrest_net::ChatKind::Player { sender } => format!("<{sender}> {}", line.text),
        stagcrest_net::ChatKind::System => line.text.clone(),
    }
}

fn line_color(line: &ChatLine, theme: &UiTheme, alpha: f32) -> Color {
    match &line.kind {
        stagcrest_net::ChatKind::Player { .. } => theme.text_primary.with_alpha(alpha),
        stagcrest_net::ChatKind::System => theme.accent.with_alpha(alpha),
    }
}

fn update_chat_display(
    mut commands: Commands,
    mut chat: ResMut<ChatUiState>,
    theme: Res<UiTheme>,
    log: Query<Entity, With<ChatLog>>,
    line_widgets: Query<Entity, With<ChatLineWidget>>,
    time: Res<Time>,
) {
    chat.messages.retain(|entry| line_alpha(entry.received_at) > 0.0);

    let needs_rebuild = chat.dirty
        || chat
            .messages
            .iter()
            .any(|entry| line_alpha(entry.received_at) < 1.0);

    if !needs_rebuild {
        return;
    }
    chat.dirty = false;

    let Ok(log_entity) = log.single() else {
        return;
    };

    for entity in &line_widgets {
        commands.entity(entity).despawn();
    }

    let _ = time;
    commands.entity(log_entity).with_children(|parent| {
        for entry in &chat.messages {
            let alpha = line_alpha(entry.received_at);
            if alpha <= 0.0 {
                continue;
            }
            let text = format_line(&entry.line);
            let color = line_color(&entry.line, &theme, alpha);
            parent.spawn((
                ChatLineWidget,
                Text::new(text),
                theme.text_font(theme.caption_size),
                TextColor(color),
            ));
        }
    });
}

fn toggle_chat_input(
    keys: Res<ButtonInput<KeyCode>>,
    editable: Query<Entity, With<EditableText>>,
    mut chat: ResMut<ChatUiState>,
    mut input_row: Query<&mut Visibility, With<ChatInputRow>>,
    input_field: Query<Entity, With<ChatInputField>>,
    mut input_focus: ResMut<InputFocus>,
    mut fly: Query<&mut FlyCamera, With<GameCamera>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    gameplay: Res<State<GameplayState>>,
) {
    if *gameplay.get() != GameplayState::Active || !keys.just_pressed(KeyCode::KeyT) {
        return;
    }
    if editable_text_has_focus(&input_focus, &editable) {
        return;
    }

    chat.input_open = true;
    for mut vis in &mut input_row {
        *vis = Visibility::Visible;
    }
    if let Ok(mut fly) = fly.single_mut() {
        if let Ok(mut c) = cursor.single_mut() {
            release_cursor(&mut fly, &mut c);
        }
    }
    if let Ok(field) = input_field.single() {
        input_focus.set(field, FocusCause::Navigated);
    }
}

fn submit_chat(
    keys: Res<ButtonInput<KeyCode>>,
    mut chat: ResMut<ChatUiState>,
    mut input_row: Query<&mut Visibility, With<ChatInputRow>>,
    mut input_field: Query<&mut EditableText, With<ChatInputField>>,
    mut input_focus: ResMut<InputFocus>,
    mut net: ResMut<GameNetClient>,
    mut fly: Query<&mut FlyCamera, With<GameCamera>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if !chat.input_open || !keys.just_pressed(KeyCode::Enter) {
        return;
    }

    let text = input_field
        .single_mut()
        .map(|field| field.value().to_string().trim().to_string())
        .unwrap_or_default();

    if !text.is_empty() {
        net.send_chat(&text);
    }

    close_chat(&mut chat, &mut input_row, &mut input_field, &mut input_focus, &mut fly, &mut cursor);
}

fn close_chat_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut chat: ResMut<ChatUiState>,
    mut input_row: Query<&mut Visibility, With<ChatInputRow>>,
    mut input_field: Query<&mut EditableText, With<ChatInputField>>,
    mut input_focus: ResMut<InputFocus>,
    mut fly: Query<&mut FlyCamera, With<GameCamera>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if !chat.input_open || !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    close_chat(
        &mut chat,
        &mut input_row,
        &mut input_field,
        &mut input_focus,
        &mut fly,
        &mut cursor,
    );
}

fn close_chat(
    chat: &mut ChatUiState,
    input_row: &mut Query<&mut Visibility, With<ChatInputRow>>,
    input_field: &mut Query<&mut EditableText, With<ChatInputField>>,
    input_focus: &mut InputFocus,
    fly: &mut Query<&mut FlyCamera, With<GameCamera>>,
    cursor: &mut Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    chat.input_open = false;
    for mut vis in input_row.iter_mut() {
        *vis = Visibility::Hidden;
    }
    if let Ok(mut field) = input_field.single_mut() {
        field.clear();
    }
    input_focus.clear();
    if let Ok(mut fly) = fly.single_mut() {
        if let Ok(mut c) = cursor.single_mut() {
            c.grab_mode = CursorGrabMode::Locked;
            c.visible = false;
            fly.captured = true;
        }
    }
}
