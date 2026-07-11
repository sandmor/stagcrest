//! Shared text-input guard for discrete character-key actions.
//!
//! ## Convention
//!
//! Keyboard bindings that react to discrete `just_pressed` **printable** keys
//! (e.g. `M` toggles the minimap, `E` the inventory, `T` the chat, hotbar
//! digits, `=` / `-` zoom) must not fire while the user is typing into a text
//! field. The single source of truth for "keyboard is busy typing" is whether
//! an [`EditableText`] widget currently holds [`InputFocus`].
//!
//! Apply the [`text_field_focused`] run condition to such systems:
//!
//! ```ignore
//! use crate::input::text_field_focused;
//! # use bevy::prelude::*;
//! # fn f(app: &mut App) {
//! app.add_systems(Update, my_toggle.run_if(not(text_field_focused)));
//! # }
//! ```
//!
//! ### Out of scope for this guard
//!
//! - **Held movement input** (`camera_system` WASD): already gated by
//!   `PlayerController::captured`, which is `false` whenever any text/overlay opens.
//!   Movement-class input keeps using that mechanism.
//! - **Function keys** (e.g. `F3` debug overlay): not typeable into text
//!   fields; no guard needed.
//! - **Overlay-open guards** (`chat.input_open`, `inventory_ui.open`): a
//!   separate concern — "don't toggle overlay B with its own key while overlay
//!   A is already open / before focus is set this frame". Those stay as in-body
//!   early returns alongside (not replaced by) this condition.
//! - **Navigation keys** (Enter, Escape): must still fire while a text field
//!   is focused, so chat can be submitted / closed; never gate these.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;

/// True when keyboard input should go to a focused [`EditableText`] widget.
pub(crate) fn editable_text_has_focus(
    input_focus: &InputFocus,
    editable: &Query<Entity, With<EditableText>>,
) -> bool {
    input_focus
        .get()
        .is_some_and(|entity| editable.contains(entity))
}

/// Run condition: true when an [`EditableText`] widget currently has
/// [`InputFocus`], i.e. the user is typing into a text field.
///
/// Apply to discrete character-key gameplay actions via
/// `.run_if(not(text_field_focused))` so bound keys do not fire while typing.
pub fn text_field_focused(
    input_focus: Res<InputFocus>,
    editable: Query<Entity, With<EditableText>>,
) -> bool {
    editable_text_has_focus(&input_focus, &editable)
}
