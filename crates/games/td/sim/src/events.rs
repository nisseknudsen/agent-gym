use crate::config::TowerKind;
use crate::world::{MobId, TowerId};

#[derive(Clone, Debug)]
pub enum TdEvent {
    TowerPlaced {
        id: TowerId,
        x: u16,
        y: u16,
        kind: TowerKind,
    },
    TowerDestroyed {
        id: TowerId,
        x: u16,
        y: u16,
    },
    MobKilled {
        id: MobId,
        x: u16,
        y: u16,
    },
    MobLeaked {
        id: MobId,
    },
    WaveStarted {
        wave: u8,
    },
    WaveEnded {
        wave: u8,
    },
    BuildQueued {
        x: u16,
        y: u16,
        kind: TowerKind,
    },
    InsufficientGold {
        cost: u32,
        have: u32,
    },
}
