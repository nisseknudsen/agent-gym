//! Match selection screen.

use bevy::prelude::*;
use crate::game::{ConnectionState, MatchInfo, MatchList, MatchStatusInfo, UiState};
use crate::networking::spectate_match;

/// Marker for the match list root.
#[derive(Component)]
pub struct MatchListRoot;

/// Marker for the match list container.
#[derive(Component)]
pub struct MatchListContainer;

/// Marker for match entry buttons.
#[derive(Component)]
pub struct MatchEntryButton {
    pub match_id: u64,
}

/// Marker for the "no matches" text.
#[derive(Component)]
pub struct NoMatchesText;

/// Spawn the match list UI hierarchy.
pub fn spawn_match_list_ui(parent: &mut ChildSpawnerCommands) {
    parent.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            position_type: PositionType::Absolute,
            ..default()
        },
        MatchListRoot,
    )).with_children(|root: &mut ChildSpawnerCommands| {
        // Title
        root.spawn((
            Text::new("TD Viewer - Select Match"),
            TextFont {
                font_size: 36.0,
                ..default()
            },
            TextColor(Color::WHITE),
            Node {
                margin: UiRect::bottom(Val::Px(30.0)),
                ..default()
            },
        ));

        // Match list container
        root.spawn((
            Node {
                width: Val::Px(400.0),
                max_height: Val::Px(400.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(15.0)),
                row_gap: Val::Px(10.0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.95)),
            MatchListContainer,
        )).with_children(|container: &mut ChildSpawnerCommands| {
            container.spawn((
                Text::new("Loading matches..."),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.6, 0.6)),
                NoMatchesText,
            ));
        });

        // Instructions
        root.spawn((
            Text::new("Click a match to spectate"),
            TextFont {
                font_size: 16.0,
                ..default()
            },
            TextColor(Color::srgb(0.5, 0.5, 0.5)),
            Node {
                margin: UiRect::top(Val::Px(20.0)),
                ..default()
            },
        ));
    });
}

/// Update the match list UI based on fetched matches.
pub fn update_match_list_ui(
    mut commands: Commands,
    match_list: Res<MatchList>,
    ui_state: Res<UiState>,
    container_query: Query<Entity, With<MatchListContainer>>,
    existing_entries: Query<Entity, With<MatchEntryButton>>,
    no_matches_query: Query<Entity, With<NoMatchesText>>,
) {
    if *ui_state != UiState::MatchSelection {
        return;
    }

    if !match_list.is_changed() {
        return;
    }

    let Ok(container) = container_query.single() else {
        return;
    };

    // Remove existing entries
    for entity in existing_entries.iter() {
        commands.entity(entity).despawn();
    }
    for entity in no_matches_query.iter() {
        commands.entity(entity).despawn();
    }

    if match_list.matches.is_empty() {
        commands.entity(container).with_children(|parent: &mut ChildSpawnerCommands| {
            parent.spawn((
                Text::new("No matches available"),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.6, 0.6)),
                NoMatchesText,
            ));
        });
        return;
    }

    // Add new entries
    commands.entity(container).with_children(|parent: &mut ChildSpawnerCommands| {
        for match_info in &match_list.matches {
            spawn_match_entry(parent, match_info);
        }
    });
}

/// Spawn a single match entry button.
fn spawn_match_entry(parent: &mut ChildSpawnerCommands, match_info: &MatchInfo) {
    let (status_text, status_color) = match &match_info.status {
        MatchStatusInfo::WaitingForPlayers { current, required } => {
            (format!("Waiting {}/{}", current, required), Color::srgb(0.8, 0.8, 0.2))
        }
        MatchStatusInfo::Running => {
            ("Running".to_string(), Color::srgb(0.3, 0.8, 0.3))
        }
        MatchStatusInfo::Finished { outcome } => {
            (format!("Finished: {}", outcome), Color::srgb(0.5, 0.5, 0.5))
        }
        MatchStatusInfo::Terminated => {
            ("Terminated".to_string(), Color::srgb(0.8, 0.3, 0.3))
        }
    };

    parent.spawn((
        Button,
        Node {
            width: Val::Percent(100.0),
            padding: UiRect::all(Val::Px(10.0)),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.2, 0.2, 0.25, 1.0)),
        MatchEntryButton { match_id: match_info.match_id },
    )).with_children(|button: &mut ChildSpawnerCommands| {
        // Match ID
        button.spawn((
            Text::new(format!("Match #{}", match_info.match_id)),
            TextFont {
                font_size: 18.0,
                ..default()
            },
            TextColor(Color::WHITE),
        ));

        // Status
        button.spawn((
            Text::new(status_text),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(status_color),
        ));
    });
}

/// Handle match selection button clicks.
pub fn handle_match_selection(
    mut interaction_query: Query<
        (&Interaction, &MatchEntryButton, &mut BackgroundColor),
        Changed<Interaction>,
    >,
    mut connection: ResMut<ConnectionState>,
    mut ui_state: ResMut<UiState>,
) {
    for (interaction, entry, mut bg_color) in interaction_query.iter_mut() {
        match *interaction {
            Interaction::Pressed => {
                spectate_match(&mut connection, &mut ui_state, entry.match_id);
                *bg_color = BackgroundColor(Color::srgba(0.4, 0.4, 0.5, 1.0));
            }
            Interaction::Hovered => {
                *bg_color = BackgroundColor(Color::srgba(0.3, 0.3, 0.35, 1.0));
            }
            Interaction::None => {
                *bg_color = BackgroundColor(Color::srgba(0.2, 0.2, 0.25, 1.0));
            }
        }
    }
}
