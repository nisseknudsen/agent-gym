use axum::Router;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpService, StreamableHttpServerConfig,
};
use sim_server::{GameServer, ServerConfig};
use sim_td::mcp::TdMcpServer;
use sim_td::TdGame;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    // Single shared game server
    let config = ServerConfig {
        default_tick_hz: 20,
        max_matches: 100,
        event_buffer_capacity: 1024,
    };
    let game_server = Arc::new(GameServer::<TdGame>::new(config));

    // MCP service in STATELESS mode (no session tracking)
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

    let app = Router::new().nest_service("/mcp", mcp_service);
    let listener = TcpListener::bind("127.0.0.1:3000").await?;

    tracing::info!("TD MCP server listening on http://127.0.0.1:3000/mcp");
    axum::serve(listener, app).await?;

    Ok(())
}
