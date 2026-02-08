//! TD Viewer Server - Serves static files and proxies MCP requests.
//!
//! This server:
//! - Serves static files (WASM app) from a dist directory
//! - Proxies /api/mcp requests to td-mcp-server:3000/mcp

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use clap::Parser;
use http_body_util::BodyExt;
use hyper::Request;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::net::TcpListener;
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

#[derive(Clone)]
struct AppState {
    mcp_server_url: String,
    http_client: Client<hyper_util::client::legacy::connect::HttpConnector, Body>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let args = Args::parse();

    // Create HTTP client for proxying
    let http_client = Client::builder(TokioExecutor::new()).build_http();

    let state = Arc::new(AppState {
        mcp_server_url: args.mcp_server.clone(),
        http_client,
    });

    // Check if static dir exists
    if !args.static_dir.exists() {
        tracing::warn!(
            "Static directory {:?} does not exist. Run `trunk build` in crates/games/td/viewer first.",
            args.static_dir
        );
    }

    // Build router
    let app = Router::new()
        // Proxy MCP requests
        .route("/api/mcp", any(proxy_mcp))
        .route("/api/mcp/{*path}", any(proxy_mcp))
        // Serve static files
        .fallback_service(ServeDir::new(&args.static_dir).append_index_html_on_directories(true))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    let listener = TcpListener::bind(addr).await?;

    tracing::info!("TD Viewer Server listening on http://{}", addr);
    tracing::info!("Proxying /api/mcp to {}/mcp", args.mcp_server);
    tracing::info!("Serving static files from {:?}", args.static_dir);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Proxy requests to the MCP server.
///
/// The MCP server uses Streamable HTTP transport which returns SSE (text/event-stream).
/// We collect the SSE response, extract the JSON data, and return it as plain JSON
/// to the WASM client (which can't handle SSE easily).
async fn proxy_mcp(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    let target_url = format!("{}/mcp", state.mcp_server_url);

    // Read the incoming request body
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

    // Build upstream request
    let mut upstream_request = Request::builder()
        .method(method)
        .uri(&uri)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(Body::from(body_bytes))
        .unwrap();

    // Set the Host header to the upstream server
    if let Some(authority) = uri.authority() {
        if let Ok(val) = HeaderValue::from_str(authority.as_str()) {
            upstream_request.headers_mut().insert("host", val);
        }
    }

    // Forward the request
    let response = match state.http_client.request(upstream_request).await {
        Ok(response) => response,
        Err(e) => {
            tracing::error!("Proxy request failed: {}", e);
            return (StatusCode::BAD_GATEWAY, format!("Proxy error: {}", e)).into_response();
        }
    };

    let (parts, body) = response.into_parts();

    // Collect the full response body
    let body_bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            tracing::error!("Failed to read response body: {}", e);
            return (StatusCode::BAD_GATEWAY, "Failed to read upstream response").into_response();
        }
    };

    let body_str = String::from_utf8_lossy(&body_bytes);

    // Check if this is an SSE response - if so, extract the JSON data
    let content_type = parts.headers.get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.contains("text/event-stream") {
        // Parse SSE: extract "data: {...}" lines
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

        // Return as plain JSON
        Response::builder()
            .status(parts.status)
            .header("Content-Type", "application/json")
            .body(Body::from(json_data))
            .unwrap()
    } else {
        // Pass through non-SSE responses as-is
        Response::from_parts(parts, Body::from(body_bytes))
    }
}
