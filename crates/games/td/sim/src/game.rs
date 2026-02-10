use crate::actions::TdAction;
use crate::config::TdConfig;
use crate::events::TdEvent;
use crate::observe::TdObservation;
use crate::pathing::compute_distance_field;
use crate::systems;
use crate::world::{TdState, WavePhase};
use sim_core::{ActionEnvelope, Game, PlayerId, TerminalOutcome, Tick};

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
    type Observation = TdObservation;
    type Event = TdEvent;

    fn new(config: Self::Config, seed: u64) -> Self {
        let mut state = TdState::new(config);
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

        // 1. Process build actions → queue builds, deduct gold
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
        TdObservation::from_state(&self.state, tick)
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
