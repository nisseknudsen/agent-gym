use crate::world::{CellState, Grid, MobId, TdState, TowerId};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Neighbor directions in fixed order: N, NE, E, SE, S, SW, W, NW
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

const CARDINAL_COST: u32 = 10;
const DIAGONAL_COST: u32 = 14;

fn is_diagonal(idx: usize) -> bool {
    idx % 2 == 1
}

fn neighbor_cost(idx: usize) -> u32 {
    if is_diagonal(idx) {
        DIAGONAL_COST
    } else {
        CARDINAL_COST
    }
}

fn diagonal_allowed(grid: &Grid, x: u16, y: u16, dx: i32, dy: i32) -> bool {
    let cx1 = (x as i32 + dx) as u16;
    let cy1 = y;
    let cx2 = x;
    let cy2 = (y as i32 + dy) as u16;

    let idx1 = grid.idx(cx1, cy1);
    let idx2 = grid.idx(cx2, cy2);

    !grid.is_blocked_idx(idx1) && !grid.is_blocked_idx(idx2)
}

/// Recompute the distance field using Dijkstra from the goal.
pub fn compute_distance_field(grid: &Grid, goal: (u16, u16), dist: &mut [u32]) {
    let width = grid.width;
    let height = grid.height;

    dist.fill(u32::MAX);

    let goal_idx = grid.idx(goal.0, goal.1);

    if grid.is_blocked_idx(goal_idx) {
        return;
    }

    let mut heap: BinaryHeap<Reverse<(u32, usize)>> = BinaryHeap::new();

    dist[goal_idx] = 0;
    heap.push(Reverse((0, goal_idx)));

    while let Some(Reverse((d, idx))) = heap.pop() {
        if d > dist[idx] {
            continue;
        }

        let x = (idx % (width as usize)) as u16;
        let y = (idx / (width as usize)) as u16;

        for (i, &(dx, dy)) in NEIGHBORS.iter().enumerate() {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;

            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }

            let nx = nx as u16;
            let ny = ny as u16;
            let nidx = grid.idx(nx, ny);

            if grid.is_blocked_idx(nidx) {
                continue;
            }

            if is_diagonal(i) && !diagonal_allowed(grid, x, y, dx, dy) {
                continue;
            }

            let cost = neighbor_cost(i);
            let new_dist = d.saturating_add(cost);

            if new_dist < dist[nidx] {
                dist[nidx] = new_dist;
                heap.push(Reverse((new_dist, nidx)));
            }
        }
    }
}

pub enum MobMoveResult {
    Moved,
    Leaked,
    AttackTower(Option<TowerId>),
}

/// Move a single mob (if can_move is true) or just determine attack target.
pub fn move_mob(state: &mut TdState, mob_id: MobId, can_move: bool) -> MobMoveResult {
    let mob = &state.world.mobs[mob_id];
    let mx = mob.x;
    let my = mob.y;
    let goal = state.config.goal;

    if mx == goal.0 && my == goal.1 {
        return MobMoveResult::Leaked;
    }

    let grid = &state.world.grid;
    let mob_cell = grid.idx(mx, my);
    let mob_dist = state.dist[mob_cell];

    if mob_dist != u32::MAX {
        let mut best_neighbor: Option<(u16, u16)> = None;
        let mut best_dist = mob_dist;

        for (i, &(dx, dy)) in NEIGHBORS.iter().enumerate() {
            let nx = mx as i32 + dx;
            let ny = my as i32 + dy;

            if nx < 0 || ny < 0 || nx >= grid.width as i32 || ny >= grid.height as i32 {
                continue;
            }

            let nx = nx as u16;
            let ny = ny as u16;
            let nidx = grid.idx(nx, ny);

            if grid.is_blocked_idx(nidx) {
                continue;
            }

            if is_diagonal(i) && !diagonal_allowed(grid, mx, my, dx, dy) {
                continue;
            }

            let nd = state.dist[nidx];
            if nd < best_dist {
                best_dist = nd;
                best_neighbor = Some((nx, ny));
            }
        }

        if let Some((nx, ny)) = best_neighbor {
            if can_move {
                state.world.mobs[mob_id].x = nx;
                state.world.mobs[mob_id].y = ny;
                return MobMoveResult::Moved;
            } else {
                return MobMoveResult::AttackTower(None);
            }
        }
    }

    // Unreachable or stuck: attack adjacent tower
    if let Some(tower_id) = find_attack_target(state, mx, my) {
        return MobMoveResult::AttackTower(Some(tower_id));
    }

    // Not adjacent to any tower — move toward nearest tower via BFS
    if can_move {
        if let Some((nx, ny)) = find_move_toward_tower(state, mx, my) {
            state.world.mobs[mob_id].x = nx;
            state.world.mobs[mob_id].y = ny;
            return MobMoveResult::Moved;
        }
    }

    MobMoveResult::AttackTower(None)
}

/// Find tower to attack using frontier heuristic. O(1) tower lookup via Grid.
fn find_attack_target(state: &TdState, mx: u16, my: u16) -> Option<TowerId> {
    let grid = &state.world.grid;
    let mut candidates: Vec<(TowerId, u32, i32, usize)> = Vec::new();

    for (i, &(dx, dy)) in NEIGHBORS.iter().enumerate() {
        let nx = mx as i32 + dx;
        let ny = my as i32 + dy;

        if nx < 0 || ny < 0 || nx >= grid.width as i32 || ny >= grid.height as i32 {
            continue;
        }

        let nx = nx as u16;
        let ny = ny as u16;

        let tower_id = match grid.get(nx, ny) {
            CellState::Tower(id) => id,
            _ => continue,
        };

        let tower = match state.world.towers.get(tower_id) {
            Some(t) => t,
            None => continue,
        };

        let score = frontier_score(grid, &state.dist, nx, ny);
        candidates.push((tower_id, score, tower.hp, i));
    }

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by_key(|&(_, score, hp, order)| (score, hp, order));
    Some(candidates[0].0)
}

/// Min distance of walkable neighbors of tower at (tx, ty).
fn frontier_score(grid: &Grid, dist: &[u32], tx: u16, ty: u16) -> u32 {
    let mut min_dist = u32::MAX;

    for &(dx, dy) in &NEIGHBORS {
        let nx = tx as i32 + dx;
        let ny = ty as i32 + dy;

        if nx < 0 || ny < 0 || nx >= grid.width as i32 || ny >= grid.height as i32 {
            continue;
        }

        let nx = nx as u16;
        let ny = ny as u16;
        let nidx = grid.idx(nx, ny);

        if !grid.is_blocked_idx(nidx) {
            let d = dist[nidx];
            if d < min_dist {
                min_dist = d;
            }
        }
    }

    min_dist
}

/// BFS to find nearest tower and return first step toward it.
fn find_move_toward_tower(state: &TdState, mx: u16, my: u16) -> Option<(u16, u16)> {
    use std::collections::VecDeque;

    let grid = &state.world.grid;
    let width = grid.width;
    let height = grid.height;
    let size = (width as usize) * (height as usize);

    let mut visited = vec![false; size];
    let mut parent: Vec<Option<usize>> = vec![None; size];
    let mut queue = VecDeque::new();

    let start_idx = grid.idx(mx, my);
    visited[start_idx] = true;
    queue.push_back(start_idx);

    let mut target_idx: Option<usize> = None;

    while let Some(idx) = queue.pop_front() {
        let x = (idx % (width as usize)) as u16;
        let y = (idx / (width as usize)) as u16;

        for (i, &(dx, dy)) in NEIGHBORS.iter().enumerate() {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;

            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }

            let nx = nx as u16;
            let ny = ny as u16;
            let nidx = grid.idx(nx, ny);

            if visited[nidx] {
                continue;
            }

            if is_diagonal(i) && !diagonal_allowed(grid, x, y, dx, dy) {
                continue;
            }

            // Found a blocked cell (tower) — this is our target
            if grid.is_blocked_idx(nidx) {
                target_idx = Some(idx);
                break;
            }

            visited[nidx] = true;
            parent[nidx] = Some(idx);
            queue.push_back(nidx);
        }

        if target_idx.is_some() {
            break;
        }
    }

    // Backtrack to find first move from start
    let mut current = target_idx?;
    while let Some(p) = parent[current] {
        if p == start_idx {
            let x = (current % (width as usize)) as u16;
            let y = (current / (width as usize)) as u16;
            return Some((x, y));
        }
        current = p;
    }

    if current != start_idx {
        let x = (current % (width as usize)) as u16;
        let y = (current / (width as usize)) as u16;
        return Some((x, y));
    }

    None
}
