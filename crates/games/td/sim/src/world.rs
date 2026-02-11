use crate::config::{TdConfig, TowerKind};
use sim_core::{PlayerId, Speed, Tick};
use slotmap::{new_key_type, SlotMap};
use std::collections::VecDeque;

new_key_type! { pub struct TowerId; }
new_key_type! { pub struct MobId; }

#[derive(Clone, Copy, Debug, Default)]
pub enum CellState {
    #[default]
    Empty,
    Building,
    Tower(TowerId),
}

impl CellState {
    pub fn is_blocked(self) -> bool {
        !matches!(self, CellState::Empty)
    }
}

#[derive(Clone, Debug)]
pub struct Grid {
    pub width: u16,
    pub height: u16,
    cells: Vec<CellState>,
}

impl Grid {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            cells: vec![CellState::Empty; (width as usize) * (height as usize)],
        }
    }

    #[inline]
    pub fn idx(&self, x: u16, y: u16) -> usize {
        (y as usize) * (self.width as usize) + (x as usize)
    }

    #[inline]
    pub fn in_bounds(&self, x: u16, y: u16) -> bool {
        x < self.width && y < self.height
    }

    #[inline]
    pub fn get(&self, x: u16, y: u16) -> CellState {
        self.cells[self.idx(x, y)]
    }

    #[inline]
    pub fn set(&mut self, x: u16, y: u16, state: CellState) {
        let idx = self.idx(x, y);
        self.cells[idx] = state;
    }

    #[inline]
    pub fn is_blocked_idx(&self, idx: usize) -> bool {
        self.cells[idx].is_blocked()
    }
}

#[derive(Clone, Debug)]
pub struct Tower {
    pub x: u16,
    pub y: u16,
    pub kind: TowerKind,
    pub hp: i32,
    pub max_hp: i32,
    pub next_fire_tick: Tick,
    pub player_id: PlayerId,
    pub upgrade_level: u8,
}

#[derive(Clone, Debug)]
pub struct Mob {
    pub x: u16,
    pub y: u16,
    pub hp: i32,
    pub dmg: i32,
    pub speed: Speed,
    pub next_move_tick: Tick,
}

#[derive(Clone, Debug)]
pub struct PendingBuild {
    pub x: u16,
    pub y: u16,
    pub kind: TowerKind,
    pub complete_tick: Tick,
    pub player_id: PlayerId,
}

#[derive(Clone, Debug)]
pub enum WavePhase {
    InWave {
        spawned: u16,
        wave_size: u16,
        next_spawn_tick: Tick,
    },
    Pause {
        until_tick: Tick,
    },
}

#[derive(Clone, Debug)]
pub struct World {
    pub towers: SlotMap<TowerId, Tower>,
    pub mobs: SlotMap<MobId, Mob>,
    pub grid: Grid,
    pub build_queue: VecDeque<PendingBuild>,
}

impl World {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            towers: SlotMap::with_key(),
            mobs: SlotMap::with_key(),
            grid: Grid::new(width, height),
            build_queue: VecDeque::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TdState {
    pub config: TdConfig,
    pub tick: Tick,
    pub world: World,
    pub current_wave: u8,
    pub phase: WavePhase,
    pub leaks: u16,
    pub dist: Vec<u32>,
    pub gold: u32,
}

impl TdState {
    pub fn new(config: TdConfig) -> Self {
        let size = (config.width as usize) * (config.height as usize);
        let gold_start = config.gold_start(config.player_count);
        let initial_pause_ticks = config.duration_to_ticks(config.inter_wave_pause);
        let world = World::new(config.width, config.height);
        Self {
            tick: 0,
            world,
            current_wave: 0,
            phase: WavePhase::Pause {
                until_tick: initial_pause_ticks,
            },
            leaks: 0,
            dist: vec![u32::MAX; size],
            gold: gold_start,
            config,
        }
    }
}
