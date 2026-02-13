use crate::actions::TdAction;
use crate::config::TdConfig;
use crate::events::TdEvent;
use crate::pathing::compute_distance_field;
use crate::systems;
use crate::world::{TdState, WavePhase};
use maze_generator::prelude::{Coordinates, Generator};
use maze_generator::recursive_backtracking::RbGenerator;
use sim_core::{ActionEnvelope, Game, PlayerId, TerminalOutcome, Tick};
use td_map_generator::dilate::{dilate_path, DilationParams};
use td_map_generator::grid::Tile;
use td_map_generator::noise::ValueNoise1D;
use td_map_generator::upscale::upscale_path;
use td_map_generator::{create_seed, solve_maze_bfs};

pub struct TdGame {
    state: TdState,
    #[allow(dead_code)]
    seed: u64,
}

impl TdGame {
    pub fn state(&self) -> &TdState {
        &self.state
    }
}

impl Game for TdGame {
    type Config = TdConfig;
    type Action = TdAction;
    type Observation = td_types::TdObservation;
    type Event = TdEvent;

    fn new(mut config: Self::Config, seed: u64) -> Self {
        let maze_size = config.maze_size;

        // 1. Generate maze
        let mut generator = RbGenerator::new(Some(create_seed(seed)));
        let mut maze = generator.generate(maze_size, maze_size).unwrap();
        maze.goal = Coordinates::new(maze_size - 1, maze_size - 1);

        // 2. Solve maze
        let path = solve_maze_bfs(&maze).expect("No maze solution found");

        // 3. Upscale path to tile grid (scale factor 3)
        let (mut tile_grid, spine) =
            upscale_path(&path, maze_size as usize, maze_size as usize);

        // 4. Compute distance field for dilation
        let dist_field = td_map_generator::distance::compute_distance_field(
            tile_grid.width,
            tile_grid.height,
            &spine,
        );

        // 5. Create noise and dilate path
        let noise = ValueNoise1D::new(seed, 256, 30.0);
        let params = DilationParams {
            base_radius: config.dilation_base_radius,
            amplitude: config.dilation_amplitude,
        };
        dilate_path(&mut tile_grid, &dist_field, &noise, &params);

        // 6. Convert TileGrid to walkable mask and extract spawn/goal
        let grid_w = tile_grid.width as u16;
        let grid_h = tile_grid.height as u16;
        let mut walkable = vec![false; tile_grid.width * tile_grid.height];
        let mut spawn = (0u16, 0u16);
        let mut goal = (grid_w - 1, grid_h - 1);

        for y in 0..tile_grid.height {
            for x in 0..tile_grid.width {
                let tile = tile_grid.get(x, y);
                let idx = y * tile_grid.width + x;
                match tile {
                    Tile::Path => walkable[idx] = true,
                    Tile::Start => {
                        walkable[idx] = true;
                        spawn = (x as u16, y as u16);
                    }
                    Tile::Goal => {
                        walkable[idx] = true;
                        goal = (x as u16, y as u16);
                    }
                    Tile::Wall => {}
                }
            }
        }

        // 7. Update config with generated map dimensions and positions
        config.width = grid_w;
        config.height = grid_h;
        config.spawn = spawn;
        config.goal = goal;

        let mut state = TdState::with_terrain(config, walkable);
        compute_distance_field(&state.world.grid, state.config.goal, &mut state.dist);
        Self { state, seed }
    }

    fn step(
        &mut self,
        tick: Tick,
        actions: &[ActionEnvelope<Self::Action>],
        out_events: &mut Vec<Self::Event>,
    ) {
        self.state.tick = tick;

        // 1. Process actions → queue builds, upgrades, deduct gold
        for action in actions {
            match &action.payload {
                TdAction::PlaceTower { x, y, kind } => {
                    systems::try_queue_build(
                        &mut self.state,
                        *x,
                        *y,
                        *kind,
                        tick,
                        action.player_id,
                        out_events,
                    );
                }
                TdAction::UpgradeTower { tower_id } => {
                    systems::try_upgrade_tower(&mut self.state, *tower_id, out_events);
                }
            }
        }

        // 2. Process completed builds → place towers
        let towers_placed = systems::process_builds(&mut self.state, tick, out_events);

        // 3. Recompute distance field if towers were placed
        if towers_placed {
            compute_distance_field(
                &self.state.world.grid,
                self.state.config.goal,
                &mut self.state.dist,
            );
        }

        // 4. Update wave phase (may spawn mobs, award gold on wave completion)
        systems::update_wave(&mut self.state, tick, out_events);

        // 5. Move mobs (mobs attack towers)
        systems::move_mobs(&mut self.state, tick, out_events);

        // 6. Tower attacks
        systems::tower_attacks(&mut self.state, tick, out_events);

        // 7. Remove dead mobs
        systems::remove_dead(&mut self.state, out_events);
    }

    fn observe(&self, tick: Tick, _player: PlayerId) -> Self::Observation {
        crate::observe::build_observation(&self.state, tick)
    }

    fn is_terminal(&self) -> Option<TerminalOutcome> {
        if self.state.leaks > self.state.config.max_leaks {
            return Some(TerminalOutcome::Lose);
        }

        if self.state.current_wave == self.state.config.waves_total {
            if let WavePhase::Pause { .. } = self.state.phase {
                if self.state.world.mobs.is_empty() {
                    return Some(TerminalOutcome::Win);
                }
            }
        }

        None
    }
}
