use super::types::*;
use crate::actions::TdAction;
use crate::config::TdConfig;
use crate::observe;
use crate::TdGame;
use rmcp::{
    ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    tool, tool_router,
};
use sim_server::{GameServer, MatchStatus, ObserveNextError, ServerConfig, SessionToken};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
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
            simulation_rate: 20,
            interaction_rate: 4,
            max_matches: 100,
            event_buffer_capacity: 1024,
        };
        let game_server = Arc::new(GameServer::<TdGame>::new(config));
        Self::new(game_server)
    }
}

const NEIGHBORS: [(i32, i32); 8] = [
    (0, -1),  // N
    (1, -1),  // NE
    (1, 0),   // E
    (1, 1),   // SE
    (0, 1),   // S
    (-1, 1),  // SW
    (-1, 0),  // W
    (-1, -1), // NW
];

/// Compute the current mob path from spawn to goal using the same Dijkstra +
/// gradient descent algorithm as the game engine.
fn compute_mob_path(obs: &TdObservation) -> Vec<Position> {
    let w = obs.map_width as usize;
    let h = obs.map_height as usize;
    let size = w * h;

    // Build blocked grid: non-walkable terrain OR tower OR pending build.
    let mut blocked = vec![false; size];
    for (i, &walkable) in obs.walkable.iter().enumerate() {
        if !walkable {
            blocked[i] = true;
        }
    }
    for t in &obs.towers {
        blocked[t.y as usize * w + t.x as usize] = true;
    }
    for b in &obs.build_queue {
        blocked[b.y as usize * w + b.x as usize] = true;
    }

    // Dijkstra from goal (matching pathing.rs exactly).
    let goal_idx = obs.goal.y as usize * w + obs.goal.x as usize;
    let spawn_idx = obs.spawn.y as usize * w + obs.spawn.x as usize;

    let mut dist = vec![u32::MAX; size];
    if blocked[goal_idx] {
        return Vec::new();
    }
    dist[goal_idx] = 0;

    let mut heap: BinaryHeap<Reverse<(u32, usize)>> = BinaryHeap::new();
    heap.push(Reverse((0, goal_idx)));

    while let Some(Reverse((d, idx))) = heap.pop() {
        if d > dist[idx] {
            continue;
        }
        // Early exit once we've settled the spawn cell.
        if idx == spawn_idx {
            break;
        }

        let x = (idx % w) as i32;
        let y = (idx / w) as i32;

        for (i, &(dx, dy)) in NEIGHBORS.iter().enumerate() {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let nidx = ny as usize * w + nx as usize;
            if blocked[nidx] {
                continue;
            }
            // Diagonal: both adjacent cardinal cells must be unblocked.
            let is_diag = i % 2 == 1;
            if is_diag {
                let cx1 = (x + dx) as usize + y as usize * w;
                let cx2 = x as usize + (y + dy) as usize * w;
                if blocked[cx1] || blocked[cx2] {
                    continue;
                }
            }
            let cost = if is_diag { 14u32 } else { 10u32 };
            let new_dist = d.saturating_add(cost);
            if new_dist < dist[nidx] {
                dist[nidx] = new_dist;
                heap.push(Reverse((new_dist, nidx)));
            }
        }
    }

    if dist[spawn_idx] == u32::MAX {
        return Vec::new();
    }

    // Gradient descent from spawn to goal.
    let mut path = vec![Position {
        x: obs.spawn.x,
        y: obs.spawn.y,
    }];
    let mut cx = obs.spawn.x;
    let mut cy = obs.spawn.y;

    loop {
        if cx == obs.goal.x && cy == obs.goal.y {
            break;
        }
        let cur_dist = dist[cy as usize * w + cx as usize];
        let mut best: Option<(u16, u16)> = None;
        let mut best_dist = cur_dist;

        for (i, &(dx, dy)) in NEIGHBORS.iter().enumerate() {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let nidx = ny as usize * w + nx as usize;
            if blocked[nidx] {
                continue;
            }
            let is_diag = i % 2 == 1;
            if is_diag {
                let cx1 = (cx as i32 + dx) as usize + cy as usize * w;
                let cx2 = cx as usize + (cy as i32 + dy) as usize * w;
                if blocked[cx1] || blocked[cx2] {
                    continue;
                }
            }
            if dist[nidx] < best_dist {
                best_dist = dist[nidx];
                best = Some((nx as u16, ny as u16));
            }
        }

        match best {
            Some((nx, ny)) => {
                path.push(Position { x: nx, y: ny });
                cx = nx;
                cy = ny;
            }
            None => break, // Stuck (shouldn't happen if dist[spawn] != MAX)
        }
    }

    path
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
                description: "A 2D grid with procedurally generated terrain. Each cell is either walkable (path) or non-walkable (wall). Mobs only travel on walkable cells. Towers can only be placed on walkable, unoccupied cells. The terrain is generated from a maze and dilated into organic paths.".to_string(),
                default_size: "30x30 cells (maze_size=10, scale factor 3)".to_string(),
                spawn_description: "Mobs spawn at the Start tile determined by map generation. Check the 'spawn' field in observations.".to_string(),
                goal_description: "Mobs try to reach the Goal tile determined by map generation. Check the 'goal' field in observations. Mobs pathfind along walkable cells around towers.".to_string(),
            },
            towers: TowerRules {
                placement: "Use the place_tower tool to queue a tower build. Towers can ONLY be placed on buildable cells (use get_buildable_cells to get them). Non-walkable cells are permanent terrain walls and cannot be built on. Cost scales with wave number (base_cost * 1.12^wave). Cell is blocked immediately when build starts.".to_string(),
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
                    name: "get_buildable_cells".to_string(),
                    description: "Get all buildable cell coordinates. The map never changes, so call this once after joining.".to_string(),
                    parameters: "match_id, session_token.".to_string(),
                },
                ActionRule {
                    name: "get_current_path".to_string(),
                    description: "Get the current shortest path mobs follow from spawn to goal. Changes when towers are built/destroyed. Call after placing towers to see the new route.".to_string(),
                    parameters: "match_id, session_token.".to_string(),
                },
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
                "RECOMMENDED START: 1) Call get_buildable_cells to learn the map layout. 2) Call get_current_path to see the mob route from spawn to goal. 3) Use this info to plan tower placements, then start building.".to_string(),
                "Use observe_next to stream game state updates. Pass after_tick=0 for the first call.".to_string(),
                "IMPORTANT: Always pass the tick value from the previous response. If you repeat the same after_tick, you WILL be forced to wait for new data - this prevents spam.".to_string(),
                "Your actions (place_tower, upgrade_tower) are independent of observe_next - submit them with intended_tick and the server will execute them.".to_string(),
                "The mob path can change when towers are placed or destroyed — mobs reroute around obstacles. Call get_current_path periodically (e.g. after a batch of tower placements) to see the updated route and adjust your strategy.".to_string(),
                "Place towers strategically on walkable cells to create longer paths - mobs will pathfind around them.".to_string(),
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
        // Pre-validate using current game state
        let obs = self
            .game_server
            .observe(params.match_id, SessionToken(params.session_token))
            .await
            .map_err(|e| format!("Failed to validate: {:?}", e))?;

        if params.x >= obs.map_width || params.y >= obs.map_height {
            return Err(format!(
                "Cannot place tower: ({},{}) is out of bounds (map is {}x{})",
                params.x, params.y, obs.map_width, obs.map_height
            ));
        }
        let idx = params.y as usize * obs.map_width as usize + params.x as usize;
        if !obs.walkable.get(idx).copied().unwrap_or(false) {
            return Err(format!(
                "Cannot place tower: ({},{}) is non-walkable terrain",
                params.x, params.y
            ));
        }
        if obs.towers.iter().any(|t| t.x == params.x && t.y == params.y)
            || obs
                .build_queue
                .iter()
                .any(|b| b.x == params.x && b.y == params.y)
        {
            return Err(format!(
                "Cannot place tower: ({},{}) is already occupied",
                params.x, params.y
            ));
        }
        if obs.gold < obs.tower_cost {
            return Err(format!(
                "Cannot place tower: insufficient gold (need {}, have {})",
                obs.tower_cost, obs.gold
            ));
        }

        let action = TdAction::PlaceTower {
            x: params.x,
            y: params.y,
            kind: observe::string_to_kind(&params.tower_type),
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
        let id = observe::string_to_tower_id(&params.tower_id)?;

        // Pre-validate using current game state
        let obs = self
            .game_server
            .observe(params.match_id, SessionToken(params.session_token))
            .await
            .map_err(|e| format!("Failed to validate: {:?}", e))?;

        let tower = obs
            .towers
            .iter()
            .find(|t| t.id == params.tower_id)
            .ok_or_else(|| {
                format!(
                    "Cannot upgrade tower: tower '{}' not found",
                    params.tower_id
                )
            })?;
        if obs.gold < tower.upgrade_cost {
            return Err(format!(
                "Cannot upgrade tower: insufficient gold (need {}, have {})",
                tower.upgrade_cost, obs.gold
            ));
        }

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

    /// Get all buildable cells on the map (static — does not change during a match).
    #[tool(description = "Get all buildable cell coordinates on the map. The map layout never changes during a match, so call this once after joining. Returns {map_width, map_height, buildable_cells: [{x, y}, ...]}.")]
    async fn get_buildable_cells(
        &self,
        Parameters(params): Parameters<GetBuildableCellsParams>,
    ) -> Result<String, String> {
        let obs = self
            .game_server
            .observe(params.match_id, SessionToken(params.session_token))
            .await
            .map_err(|e| format!("Failed to observe: {:?}", e))?;

        let mut buildable_cells = Vec::new();
        for y in 0..obs.map_height {
            for x in 0..obs.map_width {
                let idx = y as usize * obs.map_width as usize + x as usize;
                if obs.walkable.get(idx).copied().unwrap_or(false) {
                    buildable_cells.push(Position { x, y });
                }
            }
        }

        Ok(serde_json::to_string(&GetBuildableCellsResult {
            map_width: obs.map_width,
            map_height: obs.map_height,
            buildable_cells,
        })
        .unwrap())
    }

    /// Get the current mob path from spawn to goal.
    #[tool(description = "Get the current shortest path mobs will follow from spawn to goal, accounting for placed towers. The path changes when towers are built or destroyed. Returns path_exists=false if the path is fully blocked (mobs will attack towers).")]
    async fn get_current_path(
        &self,
        Parameters(params): Parameters<GetCurrentPathParams>,
    ) -> Result<String, String> {
        let obs = self
            .game_server
            .observe(params.match_id, SessionToken(params.session_token))
            .await
            .map_err(|e| format!("Failed to observe: {:?}", e))?;

        let path = compute_mob_path(&obs);
        let path_exists = !path.is_empty();

        Ok(serde_json::to_string(&GetCurrentPathResult { path_exists, path }).unwrap())
    }

    /// Wait for the next game state update (long-poll).
    #[tool(description = "Wait for game state observation. IMPORTANT: You MUST pass the tick from the previous response as after_tick, otherwise you will be forced to wait for new data (anti-spam). Use after_tick=0 for first call, then always pass the returned tick.")]
    async fn observe_next(
        &self,
        Parameters(params): Parameters<ObserveNextParams>,
    ) -> Result<String, String> {
        let (mut obs, timed_out) = self
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

        // Strip walkable array — map layout never changes, agents should use get_buildable_cells instead.
        obs.walkable.clear();

        let result = ObserveNextResult {
            timed_out,
            observation: obs,
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
