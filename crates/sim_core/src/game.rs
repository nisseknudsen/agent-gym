use crate::envelope::ActionEnvelope;
use crate::types::{PlayerId, Tick};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalOutcome {
    Win,
    Lose,
}

pub trait Game: Sized {
    type Config: Clone + Send + Sync + 'static;
    type Action: Clone + Send + Sync + 'static;
    type Observation: Clone + Send + Sync + 'static;
    type Event: Clone + Send + Sync + 'static;

    fn new(config: Self::Config, seed: u64) -> Self;

    fn step(
        &mut self,
        tick: Tick,
        actions: &[ActionEnvelope<Self::Action>],
        out_events: &mut Vec<Self::Event>,
    );

    fn observe(&self, tick: Tick, player: PlayerId) -> Self::Observation;

    fn is_terminal(&self) -> Option<TerminalOutcome>;
}
