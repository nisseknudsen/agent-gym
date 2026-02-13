//! HUD rendering: gold, wave counter, leaks display.

use bevy::prelude::*;
use crate::game::{ConnectionState, ConnectionStatus, GameStateCache, WaveStatus};

/// Marker for the HUD root.
#[derive(Component)]
pub struct HudRoot;

/// Marker for the gold display text.
#[derive(Component)]
pub struct GoldText;

/// Marker for the wave display text.
#[derive(Component)]
pub struct WaveText;

/// Marker for the leaks display text.
#[derive(Component)]
pub struct LeaksText;

/// Marker for the status display text.
#[derive(Component)]
pub struct StatusText;

/// Spawn the HUD hierarchy.
pub fn spawn_hud(parent: &mut ChildSpawnerCommands) {
    parent.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Px(60.0),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.9)),
        HudRoot,
        Visibility::Hidden,
    )).with_children(|hud: &mut ChildSpawnerCommands| {
        // Left section: Gold
        hud.spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(5.0),
                ..default()
            },
        )).with_children(|section: &mut ChildSpawnerCommands| {
            section.spawn((
                Text::new("Gold: 0"),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.85, 0.0)),
                GoldText,
            ));
        });

        // Center section: Wave info
        hud.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
        )).with_children(|section: &mut ChildSpawnerCommands| {
            section.spawn((
                Text::new("Wave 0/0"),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                WaveText,
            ));

            section.spawn((
                Text::new(""),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.7, 0.7)),
                StatusText,
            ));
        });

        // Right section: Leaks
        hud.spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(5.0),
                ..default()
            },
        )).with_children(|section: &mut ChildSpawnerCommands| {
            section.spawn((
                Text::new("Leaks: 0/0"),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.3, 0.3)),
                LeaksText,
            ));
        });
    });
}

/// Update HUD text based on game state.
pub fn update_hud(
    game_state: Res<GameStateCache>,
    connection: Res<ConnectionState>,
    mut gold_query: Query<&mut Text, (With<GoldText>, Without<WaveText>, Without<LeaksText>, Without<StatusText>)>,
    mut wave_query: Query<&mut Text, (With<WaveText>, Without<GoldText>, Without<LeaksText>, Without<StatusText>)>,
    mut leaks_query: Query<&mut Text, (With<LeaksText>, Without<GoldText>, Without<WaveText>, Without<StatusText>)>,
    mut status_query: Query<&mut Text, (With<StatusText>, Without<GoldText>, Without<WaveText>, Without<LeaksText>)>,
) {
    if !game_state.initialized {
        return;
    }

    // Update gold
    for mut text in gold_query.iter_mut() {
        **text = format!("Gold: {}", game_state.gold);
    }

    // Update wave
    for mut text in wave_query.iter_mut() {
        **text = format!("Wave {}/{}", game_state.current_wave, game_state.waves_total);
    }

    // Update leaks
    for mut text in leaks_query.iter_mut() {
        **text = format!("Leaks: {}/{}", game_state.leaks, game_state.max_leaks);
    }

    // Update status
    for mut text in status_query.iter_mut() {
        let status = match &game_state.wave_status {
            WaveStatus::Pause { until_tick, next_wave_size } if *until_tick == 0 && *next_wave_size == 0 => {
                "Connecting...".to_string()
            }
            WaveStatus::Pause { until_tick, next_wave_size } => {
                let ticks_remaining = until_tick.saturating_sub(game_state.tick);
                let seconds = ticks_remaining as f32 / game_state.ticks_per_second as f32;
                format!("Next wave ({} mobs) in {:.1}s", next_wave_size, seconds)
            }
            WaveStatus::InWave { spawned, wave_size, .. } => {
                let mobs_alive = game_state.mobs.len();
                format!("In wave: {}/{} spawned, {} alive", spawned, wave_size, mobs_alive)
            }
        };

        // Add connection status if not connected
        let final_status = match &connection.status {
            ConnectionStatus::Connected => status,
            ConnectionStatus::Connecting => "Connecting...".to_string(),
            ConnectionStatus::Disconnected => "Disconnected".to_string(),
            ConnectionStatus::Error(e) => format!("Error: {}", e),
        };

        **text = final_status;
    }
}
