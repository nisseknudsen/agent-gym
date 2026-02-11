mod hud;
mod match_list;

pub use hud::*;
pub use match_list::*;

use bevy::prelude::*;
use crate::game::{
    AttackLine, ConnectionState, DeathParticle, GameStateCache, GridCell, Mob,
    PendingBuild, Tower, UiState,
};
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
                handle_browser_back,
                toggle_ui_visibility,
                cleanup_game_entities,
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
) {
    if *ui_state == UiState::Spectating && keys.just_pressed(KeyCode::Escape) {
        leave_spectate(&mut connection, &mut game_state, &mut ui_state);
        // Push browser history so the URL reflects match selection
        push_browser_state("");
    }
}

/// Handle browser back button via popstate.
fn handle_browser_back(
    mut ui_state: ResMut<UiState>,
    mut connection: ResMut<ConnectionState>,
    mut game_state: ResMut<GameStateCache>,
) {
    if *ui_state != UiState::Spectating {
        return;
    }

    // Check if browser navigated back (hash cleared)
    let window = web_sys::window().unwrap();
    let hash = window.location().hash().unwrap_or_default();
    if hash.is_empty() || hash == "#" {
        // Only leave if we were spectating — the hash being empty means the user hit back
        if connection.match_id.is_some() {
            leave_spectate(&mut connection, &mut game_state, &mut ui_state);
        }
    }
}

/// Push a browser history state and update the URL hash.
pub fn push_browser_state(hash: &str) {
    let window = web_sys::window().unwrap();
    let history = window.history().unwrap();
    let new_url = if hash.is_empty() {
        "#".to_string()
    } else {
        format!("#{}", hash)
    };
    let _ = history.push_state_with_url(
        &wasm_bindgen::JsValue::NULL,
        "",
        Some(&new_url),
    );
}

/// Despawn all game world entities when returning to match selection.
fn cleanup_game_entities(
    mut commands: Commands,
    ui_state: Res<UiState>,
    grid_cells: Query<Entity, With<GridCell>>,
    towers: Query<Entity, With<Tower>>,
    mobs: Query<Entity, With<Mob>>,
    pending_builds: Query<Entity, With<PendingBuild>>,
    attack_lines: Query<Entity, With<AttackLine>>,
    death_particles: Query<Entity, With<DeathParticle>>,
) {
    if *ui_state != UiState::MatchSelection {
        return;
    }

    // Only run if there are entities to clean up
    if grid_cells.is_empty() && towers.is_empty() && mobs.is_empty() {
        return;
    }

    for entity in grid_cells.iter()
        .chain(towers.iter())
        .chain(mobs.iter())
        .chain(pending_builds.iter())
        .chain(attack_lines.iter())
        .chain(death_particles.iter())
    {
        commands.entity(entity).despawn();
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
