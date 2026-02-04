use super::types::*;
use crate::game::{ObsWaveStatus, TdEvent};
use crate::actions::TdAction;
use crate::state::TdConfig;
use crate::TdGame;
use rmcp::{
    ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    tool, tool_router,
};
use sim_server::{EventCursor, GameServer, MatchStatus, ServerConfig, SessionToken};
use std::sync::Arc;

/// MCP Server for the Tower Defense game.
pub struct TdMcpServer {
    game_server: Arc<GameServer<TdGame>>,
    tool_router: ToolRouter<Self>,
}

impl TdMcpServer {
    pub fn new(game_server: Arc<GameServer<TdGame>>) -> Self {
        Self {
            game_server,
            tool_router: Self::tool_router(),
        }
    }

    pub fn with_default_config() -> Self {
        let config = ServerConfig {
            default_tick_hz: 20,
            max_matches: 100,
            event_buffer_capacity: 1024,
        };
        let game_server = Arc::new(GameServer::<TdGame>::new(config));
        Self::new(game_server)
    }
}

#[tool_router]
impl TdMcpServer {
    /// Create a new Tower Defense match.
    #[tool(description = "Create a new Tower Defense match with the specified seed and player count")]
    async fn create_match(&self, Parameters(params): Parameters<CreateMatchParams>) -> Result<String, String> {
        let game_config = TdConfig {
            tick_hz: 20,
            waves_total: params.waves,
            gold_start: params.starting_gold,
            ..TdConfig::default()
        };

        let match_id = self
            .game_server
            .create_match_with_players(game_config, params.seed, params.required_players)
            .await
            .map_err(|e| format!("Failed to create match: {}", e))?;

        Ok(serde_json::to_string(&CreateMatchResult { match_id }).unwrap())
    }

    /// List all active matches.
    #[tool(description = "List all active Tower Defense matches")]
    async fn list_matches(&self) -> Result<String, String> {
        let matches = self.game_server.list_matches().await;

        let matches: Vec<_> = matches
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
            .collect();

        Ok(serde_json::to_string(&ListMatchesResult { matches }).unwrap())
    }

    /// Terminate a match.
    #[tool(description = "Terminate an active match")]
    async fn terminate_match(&self, Parameters(params): Parameters<TerminateMatchParams>) -> Result<String, String> {
        self.game_server
            .terminate_match(params.match_id)
            .await
            .map_err(|e| format!("Failed to terminate match: {}", e))?;

        Ok("Match terminated".to_string())
    }

    /// Join a match as a new player.
    #[tool(description = "Join a match as a new player. Returns a session token and player ID.")]
    async fn join_match(&self, Parameters(params): Parameters<JoinMatchParams>) -> Result<String, String> {
        let (session, player_id) = self
            .game_server
            .join_match(params.match_id)
            .await
            .map_err(|e| format!("Failed to join match: {}", e))?;

        Ok(serde_json::to_string(&JoinMatchResult {
            session_token: session.0,
            player_id,
        })
        .unwrap())
    }

    /// Leave a match.
    #[tool(description = "Leave a match")]
    async fn leave_match(&self, Parameters(params): Parameters<LeaveMatchParams>) -> Result<String, String> {
        self.game_server
            .leave_match(params.match_id, SessionToken(params.session_token))
            .await
            .map_err(|e| format!("Failed to leave match: {}", e))?;

        Ok("Left match".to_string())
    }

    /// Submit an action to the game.
    #[tool(description = "Submit an action (e.g., place a tower). If intended_tick is not provided or has passed, executes on the next tick. Returns the actual scheduled tick.")]
    async fn submit_action(&self, Parameters(params): Parameters<SubmitActionParams>) -> Result<String, String> {
        let action = match params.action {
            ActionParams::PlaceTower { x, y } => TdAction::PlaceTower { x, y, hp: 100 },
        };

        // Use intended_tick if provided, otherwise 0 (will be scheduled for next tick)
        let intended_tick = params.intended_tick.unwrap_or(0);

        let (action_id, scheduled_tick) = self
            .game_server
            .submit_action(
                params.match_id,
                SessionToken(params.session_token),
                action,
                intended_tick,
            )
            .await
            .map_err(|e| format!("Failed to submit action: {}", e))?;

        Ok(serde_json::to_string(&SubmitActionResult { action_id, scheduled_tick }).unwrap())
    }

    /// Observe the current game state.
    #[tool(description = "Get the full observation of the game state including map, entities, and wave info")]
    async fn observe(&self, Parameters(params): Parameters<ObserveParams>) -> Result<String, String> {
        let obs = self
            .game_server
            .observe(params.match_id, SessionToken(params.session_token))
            .await
            .map_err(|e| format!("Failed to observe: {}", e))?;

        let wave_status = match obs.wave_status {
            ObsWaveStatus::Pause { until_tick, next_wave_size } => WaveStatus::Pause {
                until_tick,
                next_wave_size,
            },
            ObsWaveStatus::InWave { spawned, wave_size, next_spawn_tick } => WaveStatus::InWave {
                spawned,
                wave_size,
                next_spawn_tick,
            },
        };

        Ok(serde_json::to_string(&ObserveResult {
            tick: obs.tick,
            tick_hz: obs.tick_hz,

            map_width: obs.map_width,
            map_height: obs.map_height,
            spawn: Position { x: obs.spawn.0, y: obs.spawn.1 },
            goal: Position { x: obs.goal.0, y: obs.goal.1 },

            max_leaks: obs.max_leaks,
            tower_cost: obs.tower_cost,
            tower_range: obs.tower_range,
            tower_damage: obs.tower_damage,
            build_time_ticks: obs.build_time_ticks,
            gold_per_mob_kill: obs.gold_per_mob_kill,

            gold: obs.gold,
            leaks: obs.leaks,

            current_wave: obs.current_wave,
            waves_total: obs.waves_total,
            wave_status,

            towers: obs.towers.into_iter().map(|t| TowerInfo {
                x: t.x,
                y: t.y,
                hp: t.hp,
            }).collect(),
            mobs: obs.mobs.into_iter().map(|m| MobInfo {
                x: m.x,
                y: m.y,
                hp: m.hp,
            }).collect(),
            build_queue: obs.build_queue.into_iter().map(|b| PendingBuildInfo {
                x: b.x,
                y: b.y,
                complete_tick: b.complete_tick,
            }).collect(),
        })
        .unwrap())
    }

    /// Poll events from the game.
    #[tool(description = "Poll events from the game starting at the given cursor position")]
    async fn poll_events(&self, Parameters(params): Parameters<PollEventsParams>) -> Result<String, String> {
        let (events, new_cursor) = self
            .game_server
            .poll_events(
                params.match_id,
                SessionToken(params.session_token),
                EventCursor(params.cursor),
            )
            .await
            .map_err(|e| format!("Failed to poll events: {}", e))?;

        let events: Vec<_> = events
            .into_iter()
            .map(|e| GameEvent {
                sequence: e.sequence,
                tick: e.tick,
                event: convert_event(e.event),
            })
            .collect();

        Ok(serde_json::to_string(&PollEventsResult {
            events,
            next_cursor: new_cursor.0,
        })
        .unwrap())
    }

    /// Get the current tick of a match.
    #[tool(description = "Get the current tick number of a match")]
    async fn current_tick(&self, Parameters(params): Parameters<CurrentTickParams>) -> Result<String, String> {
        let tick = self
            .game_server
            .current_tick(params.match_id)
            .await
            .map_err(|e| format!("Failed to get tick: {}", e))?;

        Ok(serde_json::to_string(&CurrentTickResult { tick }).unwrap())
    }
}

fn convert_event(event: TdEvent) -> EventData {
    match event {
        TdEvent::TowerPlaced { x, y } => EventData::TowerPlaced { x, y },
        TdEvent::TowerDestroyed { x, y } => EventData::TowerDestroyed { x, y },
        TdEvent::MobLeaked => EventData::MobLeaked,
        TdEvent::MobKilled { x, y } => EventData::MobKilled { x, y },
        TdEvent::WaveStarted { wave } => EventData::WaveStarted { wave },
        TdEvent::WaveEnded { wave } => EventData::WaveEnded { wave },
        TdEvent::BuildQueued { x, y } => EventData::BuildQueued { x, y },
        TdEvent::BuildStarted { x, y } => EventData::BuildStarted { x, y },
        TdEvent::InsufficientGold { cost, have } => EventData::InsufficientGold { cost, have },
    }
}

impl ServerHandler for TdMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Tower Defense MCP Server. Create matches, join as players, place towers, and defend against waves of mobs!".into()
            ),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            ..Default::default()
        }
    }

    fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<rmcp::model::ListToolsResult, rmcp::ErrorData>> + Send + '_ {
        let tools = self.tool_router.list_all();
        std::future::ready(Ok(rmcp::model::ListToolsResult {
            tools,
            ..Default::default()
        }))
    }

    fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, rmcp::ErrorData>> + Send + '_ {
        async move {
            let tool_context = rmcp::handler::server::tool::ToolCallContext::new(
                self,
                request,
                context,
            );
            self.tool_router.call(tool_context).await
        }
    }
}
