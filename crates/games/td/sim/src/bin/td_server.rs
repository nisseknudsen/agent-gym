//! TD Server - Combined MCP + Web/SSE server.
//!
//! Single binary that:
//! - Runs the MCP server on --mcp-port (default 3000) for AI agent connections
//! - Runs the web/SSE server on --web-port (default 8080) for browser viewers
//! - Both share the same in-process GameServer instance (no HTTP proxy overhead)

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use clap::Parser;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpService, StreamableHttpServerConfig,
};
use sim_server::{GameServer, MatchError, MatchStatus, ServerConfig, SessionToken};
use sim_td::mcp::types::*;
use sim_td::mcp::TdMcpServer;
use sim_td::TdGame;
use std::{collections::HashMap, convert::Infallible, path::PathBuf, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::RwLock};
use tokio_stream::StreamExt;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "td-server")]
#[command(about = "Combined TD MCP + Web/SSE server")]
struct Args {
    /// Port for MCP server
    #[arg(long, default_value = "3000")]
    mcp_port: u16,

    /// Port for web/SSE server
    #[arg(long, default_value = "8080")]
    web_port: u16,

    /// Static files directory (WASM app)
    #[arg(long, default_value = "crates/games/td/viewer/dist")]
    static_dir: PathBuf,
}

/// Tracks a per-match broadcast channel for SSE fan-out.
struct MatchStream {
    tx: tokio::sync::broadcast::Sender<String>,
    _task: tokio::task::JoinHandle<()>,
}

/// Tracks the match-list broadcast channel for SSE fan-out.
struct MatchListStream {
    tx: tokio::sync::broadcast::Sender<String>,
    _task: tokio::task::JoinHandle<()>,
}

struct AppState {
    game_server: Arc<GameServer<TdGame>>,
    /// Active match streams: match_id -> broadcast sender + poll task.
    streams: Arc<RwLock<HashMap<u64, MatchStream>>>,
    /// Active match-list stream: created on first subscriber, cleared when all disconnect.
    match_list_stream: Arc<RwLock<Option<MatchListStream>>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let args = Args::parse();

    // Shared game server
    let config = ServerConfig {
        default_tick_hz: 20,
        decision_hz: 4,
        max_matches: 100,
        event_buffer_capacity: 1024,
    };
    let game_server = Arc::new(GameServer::<TdGame>::new(config));

    // --- MCP server ---
    let mcp_service = StreamableHttpService::new(
        {
            let gs = game_server.clone();
            move || Ok(TdMcpServer::new(gs.clone()))
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig {
            stateful_mode: false,
            ..Default::default()
        },
    );
    let mcp_app = Router::new().nest_service("/mcp", mcp_service);

    // --- Web/SSE server ---
    let web_state = Arc::new(AppState {
        game_server: game_server.clone(),
        streams: Arc::new(RwLock::new(HashMap::new())),
        match_list_stream: Arc::new(RwLock::new(None)),
    });

    if !args.static_dir.exists() {
        tracing::warn!(
            "Static directory {:?} does not exist. Run `trunk build` in crates/games/td/viewer first.",
            args.static_dir
        );
    }

    let web_app = Router::new()
        .route("/api/stream/matches", get(stream_matches))
        .route("/api/stream/{match_id}", get(stream_match))
        .fallback_service(ServeDir::new(&args.static_dir).append_index_html_on_directories(true))
        .layer(CorsLayer::permissive())
        .with_state(web_state);

    // --- Spawn both servers ---
    let mcp_listener = TcpListener::bind(("127.0.0.1", args.mcp_port)).await?;
    let web_listener = TcpListener::bind(("0.0.0.0", args.web_port)).await?;

    tracing::info!("MCP server: http://127.0.0.1:{}/mcp", args.mcp_port);
    tracing::info!("Web server: http://0.0.0.0:{}", args.web_port);
    tracing::info!("Serving static files from {:?}", args.static_dir);

    tokio::try_join!(
        axum::serve(mcp_listener, mcp_app),
        axum::serve(web_listener, web_app),
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// SSE endpoints
// ---------------------------------------------------------------------------

/// SSE endpoint: streams the match list to all connected viewers.
async fn stream_matches(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let rx = {
        let mut lock = state.match_list_stream.write().await;

        // Check if existing stream is still alive (task may have stopped)
        let existing_alive = lock
            .as_ref()
            .map(|entry| !entry._task.is_finished())
            .unwrap_or(false);

        if existing_alive {
            let entry = lock.as_ref().unwrap();
            tracing::info!(
                "Match list SSE: new subscriber (receivers: {})",
                entry.tx.receiver_count() + 1
            );
            entry.tx.subscribe()
        } else {
            let (tx, rx) = tokio::sync::broadcast::channel::<String>(16);

            let poll_tx = tx.clone();
            let gs = state.game_server.clone();
            let mls = state.match_list_stream.clone();
            let task = tokio::spawn(async move {
                poll_match_list_loop(gs, mls, poll_tx).await;
            });

            *lock = Some(MatchListStream {
                tx: tx.clone(),
                _task: task,
            });

            tracing::info!("Match list SSE: first subscriber, started polling");
            rx
        }
    };

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).map(|result| match result {
        Ok(json) => Ok::<_, Infallible>(Event::default().data(json)),
        Err(_) => Ok(Event::default().data("{\"error\": \"stream lagged\"}")),
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// SSE endpoint: streams game state for a match to all connected viewers.
async fn stream_match(
    State(state): State<Arc<AppState>>,
    Path(match_id): Path<u64>,
) -> impl IntoResponse {
    let rx = {
        let mut streams = state.streams.write().await;

        if let Some(entry) = streams.get(&match_id) {
            tracing::info!(
                "Match {} SSE: new subscriber (receivers: {})",
                match_id,
                entry.tx.receiver_count() + 1
            );
            entry.tx.subscribe()
        } else {
            // First subscriber — create spectator session and start polling
            let (tx, rx) = tokio::sync::broadcast::channel::<String>(16);

            let session_token = match state.game_server.spectate_match(match_id).await {
                Ok(token) => token,
                Err(e) => {
                    tracing::error!("Failed to create spectator session for match {}: {}", match_id, e);
                    return (StatusCode::BAD_GATEWAY, format!("Failed to spectate match: {}", e))
                        .into_response();
                }
            };

            let poll_tx = tx.clone();
            let gs = state.game_server.clone();
            let streams_ref = state.streams.clone();
            let task = tokio::spawn(async move {
                poll_observe_loop(gs, streams_ref, match_id, session_token, poll_tx).await;
            });

            streams.insert(match_id, MatchStream {
                tx: tx.clone(),
                _task: task,
            });

            tracing::info!("Match {} SSE: first subscriber, started polling", match_id);
            rx
        }
    };

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).map(|result| match result {
        Ok(json) => Ok::<_, Infallible>(Event::default().data(json)),
        Err(_) => Ok(Event::default().data("{\"error\": \"stream lagged\"}")),
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

// ---------------------------------------------------------------------------
// Background poll loops (direct GameServer calls, no HTTP)
// ---------------------------------------------------------------------------

/// Polls `list_matches` every 2s and broadcasts to all SSE subscribers.
async fn poll_match_list_loop(
    game_server: Arc<GameServer<TdGame>>,
    match_list_stream: Arc<RwLock<Option<MatchListStream>>>,
    tx: tokio::sync::broadcast::Sender<String>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(2));

    loop {
        interval.tick().await;

        if tx.receiver_count() == 0 {
            tracing::info!("Match list SSE: no subscribers, stopping poll loop");
            break;
        }

        let matches = game_server.list_matches().await;
        let result = transform_match_list(matches);
        let json = serde_json::to_string(&result).unwrap();
        let _ = tx.send(json);
    }

    *match_list_stream.write().await = None;
    tracing::info!("Match list SSE: cleaned up stream entry");
}

/// Polls `observe` every 100ms for a match and broadcasts to all SSE subscribers.
async fn poll_observe_loop(
    game_server: Arc<GameServer<TdGame>>,
    streams: Arc<RwLock<HashMap<u64, MatchStream>>>,
    match_id: u64,
    session_token: SessionToken,
    tx: tokio::sync::broadcast::Sender<String>,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(100));

    loop {
        interval.tick().await;

        if tx.receiver_count() == 0 {
            tracing::info!("Match {} SSE: no subscribers, stopping poll loop", match_id);
            break;
        }

        match game_server.observe(match_id, session_token).await {
            Ok(obs) => {
                let json = serde_json::to_string(&obs).unwrap();
                let _ = tx.send(json);
            }
            Err(e) => {
                tracing::warn!("Match {} SSE: observe failed: {}", match_id, e);
                let _ = tx.send(format!(r#"{{"error": "{}"}}"#, e));
                if matches!(e, MatchError::NotFound) {
                    tracing::info!("Match {} SSE: match no longer exists, stopping", match_id);
                    break;
                }
            }
        }
    }

    // Cleanup: remove from streams map and leave spectator session
    streams.write().await.remove(&match_id);
    tracing::info!("Match {} SSE: cleaned up stream entry", match_id);

    let _ = game_server.leave_match(match_id, session_token).await;
}

// ---------------------------------------------------------------------------
// Transformation helpers (GameServer types → JSON-compatible types)
// ---------------------------------------------------------------------------

fn transform_match_list(matches: Vec<sim_server::MatchInfo>) -> ListMatchesResult {
    ListMatchesResult {
        matches: matches
            .into_iter()
            .map(|m| MatchInfoResult {
                match_id: m.match_id,
                status: match m.status {
                    MatchStatus::WaitingForPlayers { current, required } => {
                        MatchStatusInfo::WaitingForPlayers { current, required }
                    }
                    MatchStatus::Running => MatchStatusInfo::Running,
                    MatchStatus::Finished(outcome) => MatchStatusInfo::Finished {
                        outcome: format!("{:?}", outcome),
                    },
                    MatchStatus::Terminated => MatchStatusInfo::Terminated,
                },
                current_tick: m.current_tick,
                player_count: m.player_count,
            })
            .collect(),
    }
}
