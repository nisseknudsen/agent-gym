//! State synchronization systems.
//!
//! Game state and match list are received via SSE (Server-Sent Events) from the
//! viewer server's streaming endpoints. This enables efficient fan-out to many
//! concurrent viewers with a single upstream poll per stream.

use bevy::prelude::*;
use crate::game::{
    ConnectionState, ConnectionStatus, GameStateCache, MatchList,
    UiState, MobInfo, TowerInfo, PendingBuildInfo, WaveStatus,
    MatchInfo,
};
use crate::networking::{
    client::{stream_url, match_list_stream_url},
    SseChannel, SseConnectionState,
};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

/// Response structures from the SSE observe stream.
#[derive(Deserialize, Debug)]
struct ObserveResponse {
    tick: u64,
    ticks_per_second: u32,
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
    player_id: u8,
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
    player_id: u8,
}

#[derive(Deserialize, Debug)]
struct ListMatchesResponse {
    matches: Vec<MatchInfo>,
}

/// Manage the SSE EventSource connection for game observation.
/// Opens a connection when entering Spectating state, closes when leaving.
pub fn manage_sse_connection(
    connection: Res<ConnectionState>,
    ui_state: Res<UiState>,
    sse_channel: Res<SseChannel>,
    mut sse_state: ResMut<SseConnectionState>,
) {
    match *ui_state {
        UiState::Spectating => {
            if let Some(match_id) = connection.match_id {
                // Check if we need to open a new SSE connection
                if sse_state.connected_match_id != Some(match_id) {
                    // Close existing connection if any
                    if sse_state.is_connected {
                        close_event_source();
                        sse_state.is_connected = false;
                        sse_state.connected_match_id = None;
                    }

                    // Open new SSE connection
                    let url = stream_url(&connection.server_url, match_id);
                    let tx = sse_channel.observe_tx.clone();

                    open_event_source(&url, tx);
                    sse_state.connected_match_id = Some(match_id);
                    sse_state.is_connected = true;
                    tracing::info!("SSE: connected to match {} at {}", match_id, url);
                }
            }
        }
        UiState::MatchSelection => {
            // Close SSE connection when returning to match selection
            if sse_state.is_connected {
                close_event_source();
                sse_state.is_connected = false;
                sse_state.connected_match_id = None;
                tracing::info!("SSE: disconnected (returned to match selection)");
            }
        }
    }
}

/// Manage the SSE EventSource connection for the match list.
/// Opens when in MatchSelection state, closes when leaving.
pub fn manage_match_list_sse(
    connection: Res<ConnectionState>,
    ui_state: Res<UiState>,
    sse_channel: Res<SseChannel>,
    mut sse_state: ResMut<SseConnectionState>,
) {
    match *ui_state {
        UiState::MatchSelection => {
            if !sse_state.match_list_connected {
                let url = match_list_stream_url(&connection.server_url);
                let tx = sse_channel.match_list_tx.clone();
                open_match_list_event_source(&url, tx);
                sse_state.match_list_connected = true;
                tracing::info!("SSE: connected to match list at {}", url);
            }
        }
        UiState::Spectating => {
            if sse_state.match_list_connected {
                close_match_list_event_source();
                sse_state.match_list_connected = false;
                tracing::info!("SSE: disconnected match list (entered spectating)");
            }
        }
    }
}

/// Open an EventSource connection to the SSE stream for game observation.
fn open_event_source(url: &str, tx: crossbeam_channel::Sender<String>) {
    let url = url.to_string();

    // Use wasm_bindgen to create an EventSource in the browser
    let closure = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        if let Some(data) = event.data().as_string() {
            let _ = tx.send(data);
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);

    // Store the EventSource globally so we can close it later
    let js_code = format!(
        r#"
        if (window.__td_sse) {{
            window.__td_sse.close();
        }}
        window.__td_sse = new EventSource('{}');
        window.__td_sse.onmessage = function(e) {{
            window.__td_sse_callback(e);
        }};
        window.__td_sse.onerror = function(e) {{
            console.warn('SSE connection error, will auto-reconnect');
        }};
        "#,
        url
    );

    // Set the callback on window and eval the code
    let window = web_sys::window().unwrap();
    js_sys::Reflect::set(
        &window,
        &JsValue::from_str("__td_sse_callback"),
        closure.as_ref(),
    ).unwrap();

    // Prevent the closure from being dropped (it needs to live as long as the EventSource)
    closure.forget();

    js_sys::eval(&js_code).unwrap();
}

/// Close the active EventSource connection for game observation.
fn close_event_source() {
    let _ = js_sys::eval(
        r#"
        if (window.__td_sse) {
            window.__td_sse.close();
            window.__td_sse = null;
        }
        "#,
    );
}

/// Open an EventSource connection to the SSE stream for the match list.
fn open_match_list_event_source(url: &str, tx: crossbeam_channel::Sender<String>) {
    let url = url.to_string();

    let closure = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        if let Some(data) = event.data().as_string() {
            let _ = tx.send(data);
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);

    let js_code = format!(
        r#"
        if (window.__td_match_list_sse) {{
            window.__td_match_list_sse.close();
        }}
        window.__td_match_list_sse = new EventSource('{}');
        window.__td_match_list_sse.onmessage = function(e) {{
            window.__td_match_list_sse_callback(e);
        }};
        window.__td_match_list_sse.onerror = function(e) {{
            console.warn('Match list SSE connection error, will auto-reconnect');
        }};
        "#,
        url
    );

    let window = web_sys::window().unwrap();
    js_sys::Reflect::set(
        &window,
        &JsValue::from_str("__td_match_list_sse_callback"),
        closure.as_ref(),
    ).unwrap();

    closure.forget();

    js_sys::eval(&js_code).unwrap();
}

/// Close the active EventSource connection for the match list.
fn close_match_list_event_source() {
    let _ = js_sys::eval(
        r#"
        if (window.__td_match_list_sse) {
            window.__td_match_list_sse.close();
            window.__td_match_list_sse = null;
        }
        "#,
    );
}

/// Process SSE messages for both game observation and match list.
pub fn process_responses(
    sse_channel: Res<SseChannel>,
    mut game_state: ResMut<GameStateCache>,
    mut connection: ResMut<ConnectionState>,
    mut match_list: ResMut<MatchList>,
    mut ui_state: ResMut<UiState>,
) {
    // Process SSE observe messages (drain all available)
    while let Ok(data) = sse_channel.observe_rx.try_recv() {
        match serde_json::from_str::<ObserveResponse>(&data) {
            Ok(obs) => {
                game_state.tick = obs.tick;
                game_state.tick_hz = obs.ticks_per_second;
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
                    player_id: t.player_id,
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
                    player_id: b.player_id,
                }).collect();
                game_state.initialized = true;

                connection.status = ConnectionStatus::Connected;
            }
            Err(e) => {
                // Check if this is an error message from the server
                if data.contains("\"error\"") {
                    if data.contains("not found") {
                        tracing::info!("Match no longer exists, returning to match selection");
                        leave_spectate(&mut connection, &mut game_state, &mut ui_state);
                        return;
                    }
                    tracing::warn!("SSE error from server: {}", data);
                } else {
                    tracing::warn!("Failed to parse SSE observe data: {} - data: {}", e, data);
                }
            }
        }
    }

    // Process SSE match list messages (drain all, keep latest)
    let mut latest_match_data = None;
    while let Ok(data) = sse_channel.match_list_rx.try_recv() {
        latest_match_data = Some(data);
    }
    if let Some(data) = latest_match_data {
        match serde_json::from_str::<ListMatchesResponse>(&data) {
            Ok(list_result) => {
                match_list.matches = list_result.matches;
            }
            Err(e) => {
                tracing::warn!("Failed to parse match list SSE data: {} - data: {}", e, data);
            }
        }
    }
}

/// Start spectating a match by ID.
/// The SSE connection is managed by manage_sse_connection system.
pub fn spectate_match(
    connection: &mut ConnectionState,
    ui_state: &mut UiState,
    match_id: u64,
) {
    connection.match_id = Some(match_id);
    connection.status = ConnectionStatus::Connecting;
    *ui_state = UiState::Spectating;
    crate::ui::push_browser_state(&format!("match/{}", match_id));
    tracing::info!("Starting spectate for match {}", match_id);
}

/// Leave the current spectator session and return to match selection.
pub fn leave_spectate(
    connection: &mut ConnectionState,
    game_state: &mut GameStateCache,
    ui_state: &mut UiState,
) {
    // SSE connection will be closed by manage_sse_connection system
    // when it sees UiState change to MatchSelection

    // Reset connection state
    connection.match_id = None;
    connection.session_token = None;
    connection.status = ConnectionStatus::Disconnected;

    // Reset game state
    *game_state = GameStateCache::default();

    // Switch back to match selection
    *ui_state = UiState::MatchSelection;
}
