use crate::types::{ActionId, PlayerId, Tick};

#[derive(Clone, Debug)]
pub struct ActionEnvelope<A> {
    pub player_id: PlayerId,
    pub action_id: ActionId,
    pub intended_tick: Tick,
    pub payload: A,
}
