//! State synchronization systems.

use bevy::prelude::*;
use crate::game::{
    ConnectionState, ConnectionStatus, GameEvents, GameStateCache, MatchList,
    PollingTimers, UiState, MobInfo, TowerInfo, PendingBuildInfo, WaveStatus,
    MatchInfo,
};
use crate::networking::{
    client::{
        create_leave_match_request, create_list_matches_request, create_observe_request,
        create_poll_events_request, create_spectate_match_request, parse_tool_result,
    },
    ResponseChannels, RequestState,
};
use serde::Deserialize;

/// Response structures from MCP server.
#[derive(Deserialize, Debug)]
struct ObserveResponse {
    tick: u64,
    tick_hz: u32,
    map_width: u16,
    map_height: u16,
    spawn: Position,
    goal: Position,
    max_leaks: u16,
    tower_cost: u32,
    tower_range: u16,
    tower_damage: i32,
    build_time_ticks: u64,
    gold_per_mob_kill: u32,
    gold: u32,
    leaks: u16,
    current_wave: u8,
    waves_total: u8,
    wave_status: WaveStatusResponse,
    towers: Vec<TowerResponse>,
    mobs: Vec<MobResponse>,
    build_queue: Vec<PendingBuildResponse>,
}

#[derive(Deserialize, Debug)]
struct Position {
    x: u16,
    y: u16,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
enum WaveStatusResponse {
    Pause { until_tick: u64, next_wave_size: u16 },
    InWave { spawned: u16, wave_size: u16, next_spawn_tick: u64 },
}

#[derive(Deserialize, Debug)]
struct TowerResponse {
    x: u16,
    y: u16,
    hp: i32,
}

#[derive(Deserialize, Debug)]
struct MobResponse {
    x: u16,
    y: u16,
    hp: i32,
}

#[derive(Deserialize, Debug)]
struct PendingBuildResponse {
    x: u16,
    y: u16,
    complete_tick: u64,
}

#[derive(Deserialize, Debug)]
struct PollEventsResponse {
    events: Vec<crate::game::GameEvent>,
    next_cursor: u64,
}

#[derive(Deserialize, Debug)]
struct ListMatchesResponse {
    matches: Vec<MatchInfo>,
}

#[derive(Deserialize, Debug)]
struct SpectateMatchResponse {
    session_token: u64,
}

/// Poll the observe endpoint periodically.
pub fn poll_observe(
    time: Res<Time>,
    mut timers: ResMut<PollingTimers>,
    connection: Res<ConnectionState>,
    ui_state: Res<UiState>,
    channels: Res<ResponseChannels>,
    mut request_state: ResMut<RequestState>,
) {
    // Only poll when spectating
    if *ui_state != UiState::Spectating {
        return;
    }

    timers.observe_timer.tick(time.delta());

    if !timers.observe_timer.just_finished() {
        return;
    }

    // Don't start new request if one is pending
    if request_state.observe_pending {
        return;
    }

    // Need match_id and session_token
    let (match_id, session_token) = match (connection.match_id, connection.session_token) {
        (Some(m), Some(s)) => (m, s),
        _ => return,
    };

    request_state.observe_pending = true;
    let request = create_observe_request(&connection.server_url, match_id, session_token);
    let tx = channels.observe_tx.clone();

    ehttp::fetch(request, move |result| {
        let _ = tx.send(result.map_err(|e| e.to_string()));
    });
}

/// Poll the events endpoint periodically.
pub fn poll_events(
    time: Res<Time>,
    mut timers: ResMut<PollingTimers>,
    connection: Res<ConnectionState>,
    ui_state: Res<UiState>,
    channels: Res<ResponseChannels>,
    mut request_state: ResMut<RequestState>,
) {
    // Only poll when spectating
    if *ui_state != UiState::Spectating {
        return;
    }

    timers.events_timer.tick(time.delta());

    if !timers.events_timer.just_finished() {
        return;
    }

    // Don't start new request if one is pending
    if request_state.events_pending {
        return;
    }

    // Need match_id and session_token
    let (match_id, session_token) = match (connection.match_id, connection.session_token) {
        (Some(m), Some(s)) => (m, s),
        _ => return,
    };

    request_state.events_pending = true;
    let request = create_poll_events_request(
        &connection.server_url,
        match_id,
        session_token,
        connection.event_cursor,
    );
    let tx = channels.events_tx.clone();

    ehttp::fetch(request, move |result| {
        let _ = tx.send(result.map_err(|e| e.to_string()));
    });
}

/// Poll the match list periodically when in match selection.
pub fn poll_match_list(
    time: Res<Time>,
    connection: Res<ConnectionState>,
    ui_state: Res<UiState>,
    mut match_list: ResMut<MatchList>,
    channels: Res<ResponseChannels>,
    mut request_state: ResMut<RequestState>,
) {
    // Only poll when in match selection
    if *ui_state != UiState::MatchSelection {
        return;
    }

    // Don't start new request if one is pending
    if request_state.match_list_pending {
        return;
    }

    // Poll every 2 seconds
    let current_time = time.elapsed_secs_f64();
    if current_time - match_list.last_fetch_time < 2.0 {
        return;
    }

    match_list.last_fetch_time = current_time;
    request_state.match_list_pending = true;
    let request = create_list_matches_request(&connection.server_url);
    let tx = channels.match_list_tx.clone();

    ehttp::fetch(request, move |result| {
        let _ = tx.send(result.map_err(|e| e.to_string()));
    });
}

/// Process completed HTTP responses.
pub fn process_responses(
    channels: Res<ResponseChannels>,
    mut request_state: ResMut<RequestState>,
    mut game_state: ResMut<GameStateCache>,
    mut connection: ResMut<ConnectionState>,
    mut events: ResMut<GameEvents>,
    mut match_list: ResMut<MatchList>,
    mut ui_state: ResMut<UiState>,
) {
    // Process observe response
    if let Ok(result) = channels.observe_rx.try_recv() {
        request_state.observe_pending = false;

        match result {
            Ok(response) => {
                match parse_tool_result::<ObserveResponse>(&response) {
                    Ok(obs) => {
                        game_state.tick = obs.tick;
                        game_state.tick_hz = obs.tick_hz;
                        game_state.map_width = obs.map_width;
                        game_state.map_height = obs.map_height;
                        game_state.spawn = (obs.spawn.x, obs.spawn.y);
                        game_state.goal = (obs.goal.x, obs.goal.y);
                        game_state.max_leaks = obs.max_leaks;
                        game_state.tower_cost = obs.tower_cost;
                        game_state.tower_range = obs.tower_range;
                        game_state.tower_damage = obs.tower_damage;
                        game_state.build_time_ticks = obs.build_time_ticks;
                        game_state.gold_per_mob_kill = obs.gold_per_mob_kill;
                        game_state.gold = obs.gold;
                        game_state.leaks = obs.leaks;
                        game_state.current_wave = obs.current_wave;
                        game_state.waves_total = obs.waves_total;
                        game_state.wave_status = match obs.wave_status {
                            WaveStatusResponse::Pause { until_tick, next_wave_size } => {
                                WaveStatus::Pause { until_tick, next_wave_size }
                            }
                            WaveStatusResponse::InWave { spawned, wave_size, next_spawn_tick } => {
                                WaveStatus::InWave { spawned, wave_size, next_spawn_tick }
                            }
                        };
                        game_state.towers = obs.towers.into_iter().map(|t| TowerInfo {
                            x: t.x,
                            y: t.y,
                            hp: t.hp,
                        }).collect();
                        game_state.mobs = obs.mobs.into_iter().map(|m| MobInfo {
                            x: m.x,
                            y: m.y,
                            hp: m.hp,
                        }).collect();
                        game_state.build_queue = obs.build_queue.into_iter().map(|b| PendingBuildInfo {
                            x: b.x,
                            y: b.y,
                            complete_tick: b.complete_tick,
                        }).collect();
                        game_state.initialized = true;

                        connection.status = ConnectionStatus::Connected;
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse observe response: {}", e);
                        connection.status = ConnectionStatus::Error(e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Observe request failed: {}", e);
                connection.status = ConnectionStatus::Error(e);
            }
        }
    }

    // Process events response
    if let Ok(result) = channels.events_rx.try_recv() {
        request_state.events_pending = false;

        match result {
            Ok(response) => {
                match parse_tool_result::<PollEventsResponse>(&response) {
                    Ok(poll_result) => {
                        connection.event_cursor = poll_result.next_cursor;
                        events.events.extend(poll_result.events);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse events response: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Events request failed: {}", e);
            }
        }
    }

    // Process match list response
    if let Ok(result) = channels.match_list_rx.try_recv() {
        request_state.match_list_pending = false;

        match result {
            Ok(response) => {
                match parse_tool_result::<ListMatchesResponse>(&response) {
                    Ok(list_result) => {
                        match_list.matches = list_result.matches;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse match list response: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Match list request failed: {}", e);
            }
        }
    }

    // Process spectate match response
    if let Ok((match_id, result)) = channels.join_match_rx.try_recv() {
        request_state.join_match_pending = false;

        match result {
            Ok(response) => {
                match parse_tool_result::<SpectateMatchResponse>(&response) {
                    Ok(spectate_result) => {
                        connection.match_id = Some(match_id);
                        connection.session_token = Some(spectate_result.session_token);
                        connection.event_cursor = 0;
                        connection.status = ConnectionStatus::Connecting;
                        *ui_state = UiState::Spectating;
                        // Update browser URL so back button works
                        crate::ui::push_browser_state(&format!("match/{}", match_id));
                        tracing::info!("Spectating match {} with token {}", match_id, spectate_result.session_token);
                    }
                    Err(e) => {
                        tracing::error!("Failed to spectate match: {}", e);
                        connection.status = ConnectionStatus::Error(e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Spectate match request failed: {}", e);
                connection.status = ConnectionStatus::Error(e);
            }
        }
    }
}

/// Spectate a match by ID.
pub fn spectate_match(
    channels: &ResponseChannels,
    request_state: &mut RequestState,
    connection: &ConnectionState,
    match_id: u64,
) {
    if request_state.join_match_pending {
        return; // Already requesting
    }

    request_state.join_match_pending = true;
    let request = create_spectate_match_request(&connection.server_url, match_id);
    let tx = channels.join_match_tx.clone();

    ehttp::fetch(request, move |result| {
        let _ = tx.send((match_id, result.map_err(|e| e.to_string())));
    });
}

/// Leave the current spectator session and return to match selection.
pub fn leave_spectate(
    connection: &mut ConnectionState,
    game_state: &mut GameStateCache,
    events: &mut GameEvents,
    ui_state: &mut UiState,
) {
    // Fire-and-forget the leave request to clean up the server-side session
    if let (Some(match_id), Some(session_token)) = (connection.match_id, connection.session_token) {
        let request = create_leave_match_request(&connection.server_url, match_id, session_token);
        ehttp::fetch(request, |_| {});
    }

    // Reset connection state
    connection.match_id = None;
    connection.session_token = None;
    connection.event_cursor = 0;
    connection.status = ConnectionStatus::Disconnected;

    // Reset game state
    *game_state = GameStateCache::default();
    events.events.clear();

    // Switch back to match selection
    *ui_state = UiState::MatchSelection;
}
