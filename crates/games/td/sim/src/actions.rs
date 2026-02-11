use crate::config::TowerKind;
use crate::world::TowerId;

#[derive(Clone, Debug)]
pub enum TdAction {
    PlaceTower { x: u16, y: u16, kind: TowerKind },
    UpgradeTower { tower_id: TowerId },
}
