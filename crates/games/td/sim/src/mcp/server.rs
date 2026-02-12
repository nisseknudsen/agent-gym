use super::types::*;
use crate::actions::TdAction;
use crate::config::{TdConfig, TowerKind};
use crate::observe::ObsWaveStatus;
use crate::world::TowerId;
use crate::TdGame;
use rmcp::{
    ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    tool, tool_router,
};
use sim_server::{GameServer, MatchStatus, ObserveNextError, ServerConfig, SessionToken};
use slotmap::{Key, KeyData};
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
            decision_hz: 4,
            max_matches: 100,
            event_buffer_capacity: 1024,
        };
        let game_server = Arc::new(GameServer::<TdGame>::new(config));
        Self::new(game_server)
    }
}

fn kind_to_string(kind: TowerKind) -> String {
    match kind {
        TowerKind::Basic => "Basic".to_string(),
    }
}

fn string_to_kind(s: &str) -> TowerKind {
    match s {
        _ => TowerKind::Basic,
    }
}

fn tower_id_to_string(id: TowerId) -> String {
    id.data().as_ffi().to_string()
}

fn string_to_tower_id(s: &str) -> Result<TowerId, String> {
    let ffi: u64 = s.parse().map_err(|_| format!("Invalid tower_id: {}", s))?;
    let key_data = KeyData::from_ffi(ffi);
    Ok(TowerId::from(key_data))
}

fn transform_obs(obs: crate::observe::TdObservation) -> ObserveResult {
    let wave_status = match obs.wave_status {
        ObsWaveStatus::Pause {
            until_tick,
            next_wave_size,
        } => WaveStatus::Pause {
            until_tick,
            next_wave_size,
        },
        ObsWaveStatus::InWave {
            spawned,
            wave_size,
            next_spawn_tick,
        } => WaveStatus::InWave {
            spawned,
            wave_size,
            next_spawn_tick,
        },
    };

    ObserveResult {
        tick: obs.tick,
        ticks_per_second: obs.tick_hz,
        map_width: obs.map_width,
        map_height: obs.map_height,
        spawn: Position {
            x: obs.spawn.0,
            y: obs.spawn.1,
        },
        goal: Position {
            x: obs.goal.0,
            y: obs.goal.1,
        },
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
        towers: obs
            .towers
            .into_iter()
            .map(|t| TowerInfo {
                id: tower_id_to_string(t.id),
                x: t.x,
                y: t.y,
                hp: t.hp,
                tower_type: kind_to_string(t.kind),
                player_id: t.player_id,
                upgrade_level: t.upgrade_level,
                damage: t.damage,
                upgrade_cost: t.upgrade_cost,
            })
            .collect(),
        mobs: obs
            .mobs
            .into_iter()
            .map(|m| MobInfo {
                x: m.x,
                y: m.y,
                hp: m.hp,
            })
            .collect(),
        build_queue: obs
            .build_queue
            .into_iter()
            .map(|b| PendingBuildInfo {
                x: b.x,
                y: b.y,
                tower_type: kind_to_string(b.kind),
                complete_tick: b.complete_tick,
                player_id: b.player_id,
            })
            .collect(),
    }
}

#[tool_router]
impl TdMcpServer {
    /// Create a new Tower Defense match.
    #[tool(description = "Create a new Tower Defense match with the specified seed and player count")]
    async fn create_match(
        &self,
        Parameters(params): Parameters<CreateMatchParams>,
    ) -> Result<String, String> {
        let game_config = TdConfig {
            tick_hz: 20,
            waves_total: params.waves,
            player_count: params.required_players,
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

    /// Get the game rules and mechanics.
    #[tool(description = "Get the complete rules and mechanics of the Tower Defense game. Call this first to understand how to play.")]
    async fn rules(&self) -> Result<String, String> {
        let rules = RulesResult {
            game: "Tower Defense".to_string(),
            objective: "Defend your base by building towers to stop waves of mobs from reaching the goal. Survive all waves to win. (NOTE: You are observing only - the simulation runs at a fixed rate regardless of your calls)".to_string(),
            win_condition: "Complete all waves without exceeding the maximum number of leaks (mobs reaching the goal).".to_string(),
            lose_condition: "If more than max_leaks mobs reach the goal, you lose.".to_string(),
            map: MapRules {
                description: "A 2D grid where mobs travel from spawn to goal. Towers can be placed on any unoccupied cell.".to_string(),
                default_size: "32x32 cells".to_string(),
                spawn_description: "Mobs spawn at the spawn point (default: left side, x=0, y=16).".to_string(),
                goal_description: "Mobs try to reach the goal (default: right side, x=31, y=16). Mobs pathfind around towers.".to_string(),
            },
            towers: TowerRules {
                placement: "Use the place_tower tool to queue a tower build. Cost scales with wave number (base_cost * 1.12^wave). Cell is blocked immediately when build starts.".to_string(),
                attack: "Towers automatically attack the nearest mob within range every fire_period. Damage scales with upgrade level (base_dmg * 1.15^level).".to_string(),
                destruction: "Mobs attack adjacent towers. When a tower's HP reaches 0, it is destroyed and the cell becomes unblocked.".to_string(),
                tower_types: vec![
                    TowerTypeInfo {
                        name: "Basic".to_string(),
                        cost: 15,
                        hp: 100,
                        range: 4,
                        damage: 5,
                        description: "Standard attack tower. Base cost 15 (scales with wave). Base damage 5 (scales with upgrades). Range 4.".to_string(),
                    },
                ],
            },
            mobs: MobRules {
                movement: "Mobs spawn during waves and pathfind toward the goal, moving around towers. They take the shortest available path. If the path is completely blocked, mobs will attack towers in their way to create a path.".to_string(),
                leaking: "When a mob reaches the goal, it 'leaks' and is removed. Each leak increments the leak counter.".to_string(),
                combat: "Mobs attack towers that block their path. When adjacent to a blocking tower, they deal damage instead of moving.".to_string(),
            },
            waves: WaveRules {
                progression: "The game consists of multiple waves with exponential scaling. Mob HP and wave size grow each wave.".to_string(),
                pause_between: "There is an inter_wave_pause between waves (also before the first wave), giving you time to build and upgrade towers.".to_string(),
                scaling: "Mob HP: 10 * 1.15^wave * players. Wave size: 8 * 1.08^wave * players. Both scale linearly with player count.".to_string(),
            },
            economy: EconomyRules {
                income: "Starting gold: 50 + 30*(players-1). Wave reward: 25 * 1.12^wave * players. Kill reward: 1 * 1.08^wave. Income scales with player count.".to_string(),
                spending: "Tower build cost: base_cost * 1.12^wave. Upgrade cost: 20 * 1.20^next_level. Gold is deducted immediately.".to_string(),
            },
            actions: vec![
                ActionRule {
                    name: "place_tower".to_string(),
                    description: "Queue a tower to be built at the specified coordinates. Use the place_tower MCP tool directly.".to_string(),
                    parameters: "match_id, session_token, intended_tick, x, y, tower_type (default 'Basic').".to_string(),
                },
                ActionRule {
                    name: "upgrade_tower".to_string(),
                    description: "Upgrade a tower to increase its damage. Cost: 20 * 1.20^(current_level+1). Use the upgrade_tower MCP tool directly.".to_string(),
                    parameters: "match_id, session_token, intended_tick, tower_id (from observe response).".to_string(),
                },
            ],
            tips: vec![
                "*** CRITICAL: observe_next is READ-ONLY and DOES NOT CONTROL the simulation. The server ticks at a fixed rate regardless of your calls ***".to_string(),
                "*** Calling observe_next faster/slower does NOT speed up/slow down the game. You are only polling for state ***".to_string(),
                "Use observe_next to stream game state updates. Pass after_tick=0 for the first call.".to_string(),
                "IMPORTANT: Always pass the tick value from the previous response. If you repeat the same after_tick, you WILL be forced to wait - this prevents spam.".to_string(),
                "Your actions (place_tower, upgrade_tower) are independent of observe_next - submit them with intended_tick and the server will execute them.".to_string(),
                "Build towers near the mob path to maximize damage. Mobs walk in a straight line from spawn to goal unless blocked.".to_string(),
                "Place towers to create a maze - mobs will pathfind around them, giving towers more time to attack.".to_string(),
                "Upgrade existing towers for more damage rather than always building new ones. Upgraded towers are more gold-efficient.".to_string(),
                "Tower build cost increases each wave, so building early is cheaper.".to_string(),
                "Watch the wave_status in observe_next to know when the next wave starts and how many mobs it will have.".to_string(),
            ],
        };

        Ok(serde_json::to_string(&rules).unwrap())
    }

    /// Terminate a match.
    #[tool(description = "Terminate an active match")]
    async fn terminate_match(
        &self,
        Parameters(params): Parameters<TerminateMatchParams>,
    ) -> Result<String, String> {
        self.game_server
            .terminate_match(params.match_id)
            .await
            .map_err(|e| format!("Failed to terminate match: {}", e))?;

        Ok("Match terminated".to_string())
    }

    /// Join a match as a new player.
    #[tool(description = "Join a match as a new player. Returns a session token and player ID.")]
    async fn join_match(
        &self,
        Parameters(params): Parameters<JoinMatchParams>,
    ) -> Result<String, String> {
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
    async fn leave_match(
        &self,
        Parameters(params): Parameters<LeaveMatchParams>,
    ) -> Result<String, String> {
        self.game_server
            .leave_match(params.match_id, SessionToken(params.session_token))
            .await
            .map_err(|e| format!("Failed to leave match: {}", e))?;

        Ok("Left match".to_string())
    }

    /// Place a tower on the map.
    #[tool(description = "Place a tower at the given grid coordinates. Cost scales with wave number. If intended_tick has passed, executes on the next tick. Use 0 to execute immediately.")]
    async fn place_tower(
        &self,
        Parameters(params): Parameters<PlaceTowerParams>,
    ) -> Result<String, String> {
        let action = TdAction::PlaceTower {
            x: params.x,
            y: params.y,
            kind: string_to_kind(&params.tower_type),
        };

        let (action_id, scheduled_tick) = self
            .game_server
            .submit_action(
                params.match_id,
                SessionToken(params.session_token),
                action,
                params.intended_tick,
            )
            .await
            .map_err(|e| format!("Failed to place tower: {}", e))?;

        Ok(serde_json::to_string(&ActionResult {
            action_id,
            scheduled_tick,
        })
        .unwrap())
    }

    /// Upgrade a tower to increase its damage.
    #[tool(description = "Upgrade a tower to increase its damage. Cost: 20 * 1.20^(current_level+1). The tower_id is from the observe response.")]
    async fn upgrade_tower(
        &self,
        Parameters(params): Parameters<UpgradeTowerParams>,
    ) -> Result<String, String> {
        let id = string_to_tower_id(&params.tower_id)?;
        let action = TdAction::UpgradeTower { tower_id: id };

        let (action_id, scheduled_tick) = self
            .game_server
            .submit_action(
                params.match_id,
                SessionToken(params.session_token),
                action,
                params.intended_tick,
            )
            .await
            .map_err(|e| format!("Failed to upgrade tower: {}", e))?;

        Ok(serde_json::to_string(&ActionResult {
            action_id,
            scheduled_tick,
        })
        .unwrap())
    }

    /// Wait for the next game state update (long-poll).
    #[tool(description = "Wait for game state observation. IMPORTANT: You MUST pass the tick from the previous response as after_tick, otherwise you will be forced to wait for new data (anti-spam). Use after_tick=0 for first call, then always pass the returned tick.")]
    async fn observe_next(
        &self,
        Parameters(params): Parameters<ObserveNextParams>,
    ) -> Result<String, String> {
        let (obs, timed_out) = self
            .game_server
            .observe_next(
                params.match_id,
                SessionToken(params.session_token),
                params.after_tick,
                params.max_wait_ms,
            )
            .await
            .map_err(|e| match e {
                ObserveNextError::NotFound => "Match not found".to_string(),
                ObserveNextError::InvalidSession => "Invalid session".to_string(),
                ObserveNextError::AlreadyWaiting => {
                    "Already waiting for observation. Only one observe_next allowed at a time."
                        .to_string()
                }
                ObserveNextError::ObservationNotReady => {
                    "Observation not ready yet".to_string()
                }
            })?;

        let result = ObserveNextResult {
            timed_out,
            observation: transform_obs(obs),
        };

        Ok(serde_json::to_string(&result).unwrap())
    }

}

impl ServerHandler for TdMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Tower Defense MCP Server. Create matches, join as players, place towers, upgrade them, and defend against waves of mobs!".into()
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
    ) -> impl std::future::Future<Output = Result<rmcp::model::ListToolsResult, rmcp::ErrorData>>
           + Send
           + '_ {
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
    ) -> impl std::future::Future<Output = Result<CallToolResult, rmcp::ErrorData>> + Send + '_
    {
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
