//! TD Viewer Server - Serves static files, proxies MCP requests, and provides SSE streaming.
//!
//! This server:
//! - Serves static files (WASM app) from a dist directory
//! - Proxies /api/mcp requests to td-mcp-server:3000/mcp
//! - Provides /api/stream/{match_id} SSE endpoint for real-time game state
//!   (single upstream poll per match, fan-out to all connected viewers)

use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{any, get},
};
use clap::Parser;
use http_body_util::BodyExt;
use hyper::Request;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use std::{collections::HashMap, convert::Infallible, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{net::TcpListener, sync::RwLock};
use tokio_stream::StreamExt;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "td-viewer-server")]
#[command(about = "Web server for TD Viewer")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Directory containing static files (WASM app)
    #[arg(short, long, default_value = "crates/games/td/viewer/dist")]
    static_dir: PathBuf,

    /// MCP server URL to proxy to
    #[arg(short, long, default_value = "http://127.0.0.1:3000")]
    mcp_server: String,
}

/// Tracks a per-match broadcast channel for SSE fan-out.
struct MatchStream {
    tx: tokio::sync::broadcast::Sender<String>,
    _task: tokio::task::JoinHandle<()>,
}

struct AppState {
    mcp_server_url: String,
    http_client: Client<hyper_util::client::legacy::connect::HttpConnector, Body>,
    /// Active match streams: match_id -> broadcast sender + poll task.
    streams: RwLock<HashMap<u64, MatchStream>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let args = Args::parse();

    let http_client = Client::builder(TokioExecutor::new()).build_http();

    let state = Arc::new(AppState {
        mcp_server_url: args.mcp_server.clone(),
        http_client,
        streams: RwLock::new(HashMap::new()),
    });

    if !args.static_dir.exists() {
        tracing::warn!(
            "Static directory {:?} does not exist. Run `trunk build` in crates/games/td/viewer first.",
            args.static_dir
        );
    }

    let app = Router::new()
        .route("/api/mcp", any(proxy_mcp))
        .route("/api/mcp/{*path}", any(proxy_mcp))
        .route("/api/stream/{match_id}", get(stream_match))
        .fallback_service(ServeDir::new(&args.static_dir).append_index_html_on_directories(true))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    let listener = TcpListener::bind(addr).await?;

    tracing::info!("TD Viewer Server listening on http://{}", addr);
    tracing::info!("Proxying /api/mcp to {}/mcp", args.mcp_server);
    tracing::info!("SSE streaming at /api/stream/{{match_id}}");
    tracing::info!("Serving static files from {:?}", args.static_dir);

    axum::serve(listener, app).await?;

    Ok(())
}

/// SSE endpoint: streams game state for a match to all connected viewers.
///
/// On first subscriber for a match_id, creates a spectator session on the MCP server
/// and starts a background task that polls observe at ~100ms intervals, broadcasting
/// results to all SSE subscribers. Scales to hundreds of viewers per match with
/// a single upstream poll.
async fn stream_match(
    State(state): State<Arc<AppState>>,
    Path(match_id): Path<u64>,
) -> impl IntoResponse {
    // Get or create the broadcast channel for this match
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
            // First subscriber - create spectator session and start polling
            let (tx, rx) = tokio::sync::broadcast::channel::<String>(16);

            let session_token = match create_spectator_session(&state, match_id).await {
                Ok(token) => token,
                Err(e) => {
                    tracing::error!("Failed to create spectator session for match {}: {}", match_id, e);
                    return (StatusCode::BAD_GATEWAY, format!("Failed to spectate match: {}", e))
                        .into_response();
                }
            };

            let poll_tx = tx.clone();
            let poll_state = state.clone();
            let task = tokio::spawn(async move {
                poll_observe_loop(poll_state, match_id, session_token, poll_tx).await;
            });

            streams.insert(match_id, MatchStream {
                tx: tx.clone(),
                _task: task,
            });

            tracing::info!("Match {} SSE: first subscriber, started polling", match_id);
            rx
        }
    };

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
        .map(|result| {
            match result {
                Ok(json) => Ok::<_, Infallible>(Event::default().data(json)),
                Err(_) => Ok(Event::default().data("{\"error\": \"stream lagged\"}")),
            }
        });

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Create a spectator session on the MCP server.
async fn create_spectator_session(state: &AppState, match_id: u64) -> Result<u64, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "spectate_match",
            "arguments": {
                "match_id": match_id
            }
        }
    });

    let json = call_mcp(state, &body).await?;

    // Parse the tool result: result.content[0].text -> JSON with session_token
    let text = json
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("text"))
        .and_then(|t| t.as_str())
        .ok_or_else(|| "Missing text in spectate response".to_string())?;

    let parsed: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| format!("Failed to parse spectate result: {}", e))?;

    parsed
        .get("session_token")
        .and_then(|t| t.as_u64())
        .ok_or_else(|| "Missing session_token in spectate result".to_string())
}

/// Background task: polls the MCP observe endpoint and broadcasts results.
/// Stops automatically when all SSE subscribers disconnect (receiver_count drops to 0).
/// On exit, removes itself from the streams map and leaves the spectator session.
async fn poll_observe_loop(
    state: Arc<AppState>,
    match_id: u64,
    session_token: u64,
    tx: tokio::sync::broadcast::Sender<String>,
) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));

    loop {
        interval.tick().await;

        // If no subscribers left, stop polling
        if tx.receiver_count() == 0 {
            tracing::info!("Match {} SSE: no subscribers, stopping poll loop", match_id);
            break;
        }

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "observe",
                "arguments": {
                    "match_id": match_id,
                    "session_token": session_token
                }
            }
        });

        match call_mcp(&state, &body).await {
            Ok(json) => {
                let text = json
                    .get("result")
                    .and_then(|r| r.get("content"))
                    .and_then(|c| c.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|item| item.get("text"))
                    .and_then(|t| t.as_str());

                if let Some(observe_json) = text {
                    let _ = tx.send(observe_json.to_string());
                } else {
                    tracing::warn!("Match {} SSE: unexpected observe response format", match_id);
                }
            }
            Err(e) => {
                tracing::warn!("Match {} SSE: observe failed: {}", match_id, e);
                let _ = tx.send(format!("{{\"error\": \"{}\"}}", e.replace('"', "\\\"")));
                if e.contains("not found") {
                    tracing::info!("Match {} SSE: match no longer exists, stopping", match_id);
                    break;
                }
            }
        }
    }

    // Remove from streams map
    state.streams.write().await.remove(&match_id);
    tracing::info!("Match {} SSE: cleaned up stream entry", match_id);

    // Leave the spectator session on the MCP server
    let leave_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "leave_match",
            "arguments": {
                "match_id": match_id,
                "session_token": session_token
            }
        }
    });
    let _ = call_mcp(&state, &leave_body).await;
}

/// Make a JSON-RPC call to the MCP server and return the parsed response.
async fn call_mcp(
    state: &AppState,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let target_url = format!("{}/mcp", state.mcp_server_url);
    let uri: hyper::Uri = target_url
        .parse()
        .map_err(|e| format!("Invalid URI: {}", e))?;

    let body_bytes = serde_json::to_vec(body).unwrap();

    let mut request = Request::builder()
        .method("POST")
        .uri(&uri)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(Body::from(body_bytes))
        .unwrap();

    if let Some(authority) = uri.authority() {
        if let Ok(val) = HeaderValue::from_str(authority.as_str()) {
            request.headers_mut().insert("host", val);
        }
    }

    let response = state
        .http_client
        .request(request)
        .await
        .map_err(|e| format!("MCP request failed: {}", e))?;

    let (parts, body) = response.into_parts();
    let body_bytes = body
        .collect()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?
        .to_bytes();
    let body_str = String::from_utf8_lossy(&body_bytes);

    let content_type = parts
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let json_str = if content_type.contains("text/event-stream") {
        // Parse SSE: extract "data: {...}" line
        body_str
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .ok_or_else(|| "No data in SSE response".to_string())?
            .to_string()
    } else {
        body_str.to_string()
    };

    serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse JSON: {} - body: {}", e, json_str))
}

/// Proxy requests to the MCP server.
///
/// The MCP server uses Streamable HTTP transport which may return SSE.
/// We collect the SSE response, extract the JSON data, and return it as plain JSON
/// to the WASM client.
async fn proxy_mcp(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    let target_url = format!("{}/mcp", state.mcp_server_url);

    let method = request.method().clone();
    let body_bytes = match request.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            tracing::error!("Failed to read request body: {}", e);
            return (StatusCode::BAD_REQUEST, "Failed to read request body").into_response();
        }
    };

    let uri = match target_url.parse::<hyper::Uri>() {
        Ok(uri) => uri,
        Err(e) => {
            tracing::error!("Failed to parse target URI: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Invalid target URI").into_response();
        }
    };

    let mut upstream_request = Request::builder()
        .method(method)
        .uri(&uri)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(Body::from(body_bytes))
        .unwrap();

    if let Some(authority) = uri.authority() {
        if let Ok(val) = HeaderValue::from_str(authority.as_str()) {
            upstream_request.headers_mut().insert("host", val);
        }
    }

    let response = match state.http_client.request(upstream_request).await {
        Ok(response) => response,
        Err(e) => {
            tracing::error!("Proxy request failed: {}", e);
            return (StatusCode::BAD_GATEWAY, format!("Proxy error: {}", e)).into_response();
        }
    };

    let (parts, body) = response.into_parts();

    let body_bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            tracing::error!("Failed to read response body: {}", e);
            return (StatusCode::BAD_GATEWAY, "Failed to read upstream response").into_response();
        }
    };

    let body_str = String::from_utf8_lossy(&body_bytes);

    let content_type = parts
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.contains("text/event-stream") {
        let mut json_data = String::new();
        for line in body_str.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                json_data = data.to_string();
                break;
            }
        }

        if json_data.is_empty() {
            return (StatusCode::BAD_GATEWAY, "No data in SSE response").into_response();
        }

        Response::builder()
            .status(parts.status)
            .header("Content-Type", "application/json")
            .body(Body::from(json_data))
            .unwrap()
    } else {
        Response::from_parts(parts, Body::from(body_bytes))
    }
}
