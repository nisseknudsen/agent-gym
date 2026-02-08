use crate::state::TdState;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Neighbor directions in fixed order: N, NE, E, SE, S, SW, W, NW
/// Stored as (dx, dy) where positive x is right, positive y is down
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

/// Costs: cardinal = 10, diagonal = 14 (approximates sqrt(2) * 10)
const CARDINAL_COST: u32 = 10;
const DIAGONAL_COST: u32 = 14;

fn is_diagonal(idx: usize) -> bool {
    // NE=1, SE=3, SW=5, NW=7 are diagonal
    idx % 2 == 1
}

fn neighbor_cost(idx: usize) -> u32 {
    if is_diagonal(idx) {
        DIAGONAL_COST
    } else {
        CARDINAL_COST
    }
}

/// Check if a diagonal move is allowed (no corner cutting)
fn diagonal_allowed(state: &TdState, x: u16, y: u16, dx: i32, dy: i32) -> bool {
    // The two adjacent cardinal cells must be walkable
    let cx1 = (x as i32 + dx) as u16;
    let cy1 = y;
    let cx2 = x;
    let cy2 = (y as i32 + dy) as u16;

    let idx1 = state.idx(cx1, cy1);
    let idx2 = state.idx(cx2, cy2);

    !state.blocked[idx1] && !state.blocked[idx2]
}

/// Recompute the distance field using Dijkstra from the goal
pub fn compute_distance_field(state: &mut TdState) {
    let width = state.config.width;
    let height = state.config.height;

    // Reset distances to infinity
    state.dist.fill(u32::MAX);

    let goal = state.config.goal;
    let goal_idx = state.idx(goal.0, goal.1);

    // If goal is blocked, no path exists
    if state.blocked[goal_idx] {
        return;
    }

    // Min-heap: (distance, index)
    let mut heap: BinaryHeap<Reverse<(u32, usize)>> = BinaryHeap::new();

    state.dist[goal_idx] = 0;
    heap.push(Reverse((0, goal_idx)));

    while let Some(Reverse((d, idx))) = heap.pop() {
        // Skip if we've already found a better path
        if d > state.dist[idx] {
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
            let nidx = state.idx(nx, ny);

            // Can't move to blocked cell
            if state.blocked[nidx] {
                continue;
            }

            // Check diagonal constraint
            if is_diagonal(i) && !diagonal_allowed(state, x, y, dx, dy) {
                continue;
            }

            let cost = neighbor_cost(i);
            let new_dist = d.saturating_add(cost);

            if new_dist < state.dist[nidx] {
                state.dist[nidx] = new_dist;
                heap.push(Reverse((new_dist, nidx)));
            }
        }
    }
}

/// Move a single mob (if can_move is true) or just determine attack target
/// Returns Leaked if at goal, Moved if moved, AttackTower if attacking or stuck
pub fn move_mob(state: &mut TdState, mob_idx: usize, can_move: bool) -> MobMoveResult {
    let mob = &state.mobs[mob_idx];
    let mx = mob.x;
    let my = mob.y;
    let goal = state.config.goal;

    // Check if at goal (always check, even on non-move ticks)
    if mx == goal.0 && my == goal.1 {
        return MobMoveResult::Leaked;
    }

    let mob_cell = state.idx(mx, my);
    let mob_dist = state.dist[mob_cell];

    // If reachable (not INF), try to follow the gradient
    if mob_dist != u32::MAX {
        let mut best_neighbor: Option<(u16, u16)> = None;
        let mut best_dist = mob_dist;

        for (i, &(dx, dy)) in NEIGHBORS.iter().enumerate() {
            let nx = mx as i32 + dx;
            let ny = my as i32 + dy;

            if nx < 0
                || ny < 0
                || nx >= state.config.width as i32
                || ny >= state.config.height as i32
            {
                continue;
            }

            let nx = nx as u16;
            let ny = ny as u16;
            let nidx = state.idx(nx, ny);

            // Can't move to blocked cell
            if state.blocked[nidx] {
                continue;
            }

            // Check diagonal constraint
            if is_diagonal(i) && !diagonal_allowed(state, mx, my, dx, dy) {
                continue;
            }

            let nd = state.dist[nidx];
            // Must be strictly smaller to move towards goal
            if nd < best_dist {
                best_dist = nd;
                best_neighbor = Some((nx, ny));
            }
        }

        if let Some((nx, ny)) = best_neighbor {
            if can_move {
                state.mobs[mob_idx].x = nx;
                state.mobs[mob_idx].y = ny;
                return MobMoveResult::Moved;
            } else {
                // Can move but not a move tick - do nothing
                return MobMoveResult::AttackTower(None);
            }
        }
    }

    // Unreachable or stuck: check if we can attack an adjacent tower
    // Attacks happen every tick regardless of can_move
    if let Some(tower_idx) = find_attack_target(state, mx, my) {
        return MobMoveResult::AttackTower(Some(tower_idx));
    }

    // Not adjacent to any tower - move toward nearest blocked cell (tower)
    if can_move {
        if let Some((nx, ny)) = find_move_toward_tower(state, mx, my) {
            state.mobs[mob_idx].x = nx;
            state.mobs[mob_idx].y = ny;
            return MobMoveResult::Moved;
        }
    }

    // Can't do anything
    MobMoveResult::AttackTower(None)
}

pub enum MobMoveResult {
    Moved,
    Leaked,
    AttackTower(Option<usize>), // tower index to attack
}

/// Find tower to attack using frontier heuristic
fn find_attack_target(state: &TdState, mx: u16, my: u16) -> Option<usize> {
    let mut candidates: Vec<(usize, u32, i32, usize)> = Vec::new(); // (tower_idx, score, hp, neighbor_order)

    for (i, &(dx, dy)) in NEIGHBORS.iter().enumerate() {
        let nx = mx as i32 + dx;
        let ny = my as i32 + dy;

        if nx < 0
            || ny < 0
            || nx >= state.config.width as i32
            || ny >= state.config.height as i32
        {
            continue;
        }

        let nx = nx as u16;
        let ny = ny as u16;
        let nidx = state.idx(nx, ny);

        // Only consider blocked cells with towers
        if !state.blocked[nidx] {
            continue;
        }

        // Find the tower at this position
        let Some(tower_idx) = state.towers.iter().position(|t| t.x == nx && t.y == ny) else {
            continue;
        };

        let tower = &state.towers[tower_idx];

        // Calculate frontier score: min dist of walkable neighbors of this tower
        let score = frontier_score(state, nx, ny);

        candidates.push((tower_idx, score, tower.hp, i));
    }

    if candidates.is_empty() {
        return None;
    }

    // Sort by: score (lower is better), then HP (lower is better), then neighbor order
    candidates.sort_by_key(|&(_, score, hp, order)| (score, hp, order));

    Some(candidates[0].0)
}

/// Calculate frontier score for a tower: min dist of its walkable neighbors
fn frontier_score(state: &TdState, tx: u16, ty: u16) -> u32 {
    let mut min_dist = u32::MAX;

    for &(dx, dy) in &NEIGHBORS {
        let nx = tx as i32 + dx;
        let ny = ty as i32 + dy;

        if nx < 0
            || ny < 0
            || nx >= state.config.width as i32
            || ny >= state.config.height as i32
        {
            continue;
        }

        let nx = nx as u16;
        let ny = ny as u16;
        let nidx = state.idx(nx, ny);

        if !state.blocked[nidx] {
            let d = state.dist[nidx];
            if d < min_dist {
                min_dist = d;
            }
        }
    }

    min_dist
}

/// Find a move toward the nearest tower (blocked cell) using BFS
/// Used when mob is unreachable and not adjacent to any tower
fn find_move_toward_tower(state: &TdState, mx: u16, my: u16) -> Option<(u16, u16)> {
    use std::collections::VecDeque;

    let width = state.config.width;
    let height = state.config.height;
    let size = (width as usize) * (height as usize);

    // BFS to find nearest blocked cell
    let mut visited = vec![false; size];
    let mut parent: Vec<Option<usize>> = vec![None; size];
    let mut queue = VecDeque::new();

    let start_idx = state.idx(mx, my);
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
            let nidx = state.idx(nx, ny);

            if visited[nidx] {
                continue;
            }

            // Check diagonal constraint from current cell
            if is_diagonal(i) && !diagonal_allowed(state, x, y, dx, dy) {
                continue;
            }

            // Found a blocked cell (tower) - this is our target
            if state.blocked[nidx] {
                target_idx = Some(idx); // Parent of the blocked cell
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

    // If current's parent is None but current != start, current is adjacent to start
    if current != start_idx {
        let x = (current % (width as usize)) as u16;
        let y = (current / (width as usize)) as u16;
        return Some((x, y));
    }

    None
}
