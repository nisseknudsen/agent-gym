use crate::config::TowerKind;

#[derive(Clone, Debug)]
pub enum TdAction {
    PlaceTower { x: u16, y: u16, kind: TowerKind },
}
