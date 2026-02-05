mod hud;
mod match_list;

pub use hud::*;
pub use match_list::*;

use bevy::prelude::*;
use crate::game::{ConnectionState, GameEvents, GameStateCache, UiState};
use crate::networking::leave_spectate;

/// Plugin for UI systems.
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::game::UiState>()
            .add_systems(Startup, setup_ui)
            .add_systems(Update, (
                update_hud,
                update_match_list_ui,
                handle_match_selection,
                handle_leave_spectate,
                toggle_ui_visibility,
            ));
    }
}

/// Marker for the root UI node.
#[derive(Component)]
pub struct UiRoot;

/// Set up the UI hierarchy.
fn setup_ui(mut commands: Commands) {
    // Root UI container
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        UiRoot,
    )).with_children(|parent: &mut ChildSpawnerCommands| {
        // HUD at the top
        spawn_hud(parent);

        // Match list (centered, shows when in selection mode)
        spawn_match_list_ui(parent);
    });
}

/// Handle Escape key to leave spectator mode and return to match selection.
fn handle_leave_spectate(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui_state: ResMut<UiState>,
    mut connection: ResMut<ConnectionState>,
    mut game_state: ResMut<GameStateCache>,
    mut events: ResMut<GameEvents>,
) {
    if *ui_state == UiState::Spectating && keys.just_pressed(KeyCode::Escape) {
        leave_spectate(&mut connection, &mut game_state, &mut events, &mut ui_state);
    }
}

/// Toggle visibility of UI elements based on state.
fn toggle_ui_visibility(
    ui_state: Res<crate::game::UiState>,
    mut hud_query: Query<&mut Visibility, (With<HudRoot>, Without<MatchListRoot>)>,
    mut match_list_query: Query<&mut Visibility, (With<MatchListRoot>, Without<HudRoot>)>,
) {
    let show_hud = *ui_state == crate::game::UiState::Spectating;
    let show_match_list = *ui_state == crate::game::UiState::MatchSelection;

    for mut visibility in hud_query.iter_mut() {
        *visibility = if show_hud { Visibility::Visible } else { Visibility::Hidden };
    }

    for mut visibility in match_list_query.iter_mut() {
        *visibility = if show_match_list { Visibility::Visible } else { Visibility::Hidden };
    }
}
