use sim_core::{ActionEnvelope, Game, PlayerId, TerminalOutcome, Tick};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct RunResult<G: Game> {
    pub outcome: Option<TerminalOutcome>,
    pub final_tick: Tick,
    pub events: Vec<G::Event>,
}

pub struct MatchHost<G: Game> {
    game: G,
    current_tick: Tick,
    tick_hz: u32,
    next_player_id: PlayerId,
    pending_actions: BTreeMap<Tick, Vec<ActionEnvelope<G::Action>>>,
}

impl<G: Game> MatchHost<G> {
    pub fn new(config: G::Config, seed: u64, tick_hz: u32) -> Self {
        Self {
            game: G::new(config, seed),
            current_tick: 0,
            tick_hz,
            next_player_id: 0,
            pending_actions: BTreeMap::new(),
        }
    }

    pub fn join_player(&mut self) -> PlayerId {
        let id = self.next_player_id;
        self.next_player_id += 1;
        id
    }

    /// Submit an action to be executed at the given tick.
    /// If `intended_tick` is None or in the past, schedules for the next tick.
    /// Returns the actual tick the action was scheduled for.
    pub fn submit(&mut self, mut action: ActionEnvelope<G::Action>) -> Tick {
        // If intended tick is in the past or current, schedule for next tick
        let scheduled_tick = if action.intended_tick <= self.current_tick {
            self.current_tick + 1
        } else {
            action.intended_tick
        };

        action.intended_tick = scheduled_tick;
        self.pending_actions
            .entry(scheduled_tick)
            .or_default()
            .push(action);

        scheduled_tick
    }

    pub fn run_for_ticks(&mut self, max_ticks: Tick) -> RunResult<G> {
        let mut all_events = Vec::new();

        for _ in 0..max_ticks {
            // Check terminal before advancing
            if let Some(outcome) = self.game.is_terminal() {
                return RunResult {
                    outcome: Some(outcome),
                    final_tick: self.current_tick,
                    events: all_events,
                };
            }

            // Increment tick
            self.current_tick += 1;

            // Extract actions for this tick
            let mut actions = self
                .pending_actions
                .remove(&self.current_tick)
                .unwrap_or_default();

            // Sort by (player_id, action_id) for determinism
            actions.sort_by_key(|a| (a.player_id, a.action_id));

            // Step the game
            let mut tick_events = Vec::new();
            self.game
                .step(self.current_tick, &actions, &mut tick_events);
            all_events.extend(tick_events);
        }

        // Check terminal one final time
        let outcome = self.game.is_terminal();
        RunResult {
            outcome,
            final_tick: self.current_tick,
            events: all_events,
        }
    }

    /// Advance by one tick. Returns None if game already terminal, otherwise the events from this tick.
    pub fn step_one_tick(&mut self) -> Option<Vec<G::Event>> {
        // Check terminal before advancing
        if self.game.is_terminal().is_some() {
            return None;
        }

        // Increment tick
        self.current_tick += 1;

        // Extract actions for this tick
        let mut actions = self
            .pending_actions
            .remove(&self.current_tick)
            .unwrap_or_default();

        // Sort by (player_id, action_id) for determinism
        actions.sort_by_key(|a| (a.player_id, a.action_id));

        // Step the game
        let mut tick_events = Vec::new();
        self.game
            .step(self.current_tick, &actions, &mut tick_events);

        Some(tick_events)
    }

    pub fn game(&self) -> &G {
        &self.game
    }

    pub fn current_tick(&self) -> Tick {
        self.current_tick
    }

    pub fn tick_hz(&self) -> u32 {
        self.tick_hz
    }

    pub fn is_terminal(&self) -> Option<TerminalOutcome> {
        self.game.is_terminal()
    }
}
