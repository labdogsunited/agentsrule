//! Other entities within the interaction radius, from the snapshot (spec §5.5, §6.1).
//!
//! Positions are binned into square buckets at least `max_interaction_radius` wide, so
//! every neighbour of a point lies in its own bucket or the eight around it.

use crate::world::Space;

#[derive(Debug, Clone, Default)]
pub struct Index {
    radius: f32,
    side: [f32; 2],
    buckets: [usize; 2],
    /// Entity indices grouped by bucket; bucket `b` holds `order[start[b]..start[b + 1]]`.
    start: Vec<u32>,
    order: Vec<u32>,
    /// Contract mode (`contract.rs`): distances use the fused form.
    pub contract: bool,
}

/// A neighbour: squared distance, entity ID and index into `World::entities`.
pub type Near = (f32, u64, usize);

impl Index {
    /// Bins `positions` (in entity order). An empty index finds no one.
    pub fn build(space: &Space, positions: impl ExactSizeIterator<Item = [f32; 2]> + Clone) -> Index {
        if space.radius <= 0.0 {
            return Index::default();
        }
        let bw = ((space.w / space.radius).floor() as usize).max(1);
        let bh = ((space.h / space.radius).floor() as usize).max(1);
        let mut ix = Index { radius: space.radius, side: [space.w / bw as f32, space.h / bh as f32], buckets: [bw, bh], ..Index::default() };
        let mut count = vec![0u32; bw * bh + 1];
        for p in positions.clone() {
            count[ix.bucket(p) + 1] += 1;
        }
        for b in 1..count.len() {
            count[b] += count[b - 1];
        }
        let mut fill = count.clone();
        ix.order = vec![0; positions.len()];
        for (i, p) in positions.enumerate() {
            let b = ix.bucket(p);
            ix.order[fill[b] as usize] = i as u32;
            fill[b] += 1;
        }
        ix.start = count;
        ix
    }

    fn bucket(&self, p: [f32; 2]) -> usize {
        let x = ((p[0] / self.side[0]) as usize).min(self.buckets[0] - 1);
        let y = ((p[1] / self.side[1]) as usize).min(self.buckets[1] - 1);
        y * self.buckets[0] + x
    }

    /// The entities other than `me` within `radius` (at most the interaction radius) of
    /// `p`, nearest first, ties by lower ID. Distances wrap if the space does.
    pub fn within(&self, space: &Space, ids: &[u64], positions: &[[f32; 2]], me: usize, radius: f32, out: &mut Vec<Near>) {
        out.clear();
        if self.start.is_empty() {
            return;
        }
        let r2 = { let r = radius.min(self.radius); r * r };
        let p = positions[me];
        let b = self.bucket(p);
        let (bx, by) = ((b % self.buckets[0]) as isize, (b / self.buckets[0]) as isize);
        let mut seen: [usize; 9] = [usize::MAX; 9];
        let mut n = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (mut x, mut y) = (bx + dx, by + dy);
                let (w, h) = (self.buckets[0] as isize, self.buckets[1] as isize);
                if space.wrap {
                    (x, y) = (x.rem_euclid(w), y.rem_euclid(h));
                } else if !(0..w).contains(&x) || !(0..h).contains(&y) {
                    continue;
                }
                let k = (y * w + x) as usize;
                if seen[..n].contains(&k) {
                    continue;
                }
                seen[n] = k;
                n += 1;
                for &j in &self.order[self.start[k] as usize..self.start[k + 1] as usize] {
                    let j = j as usize;
                    if j == me {
                        continue;
                    }
                    let d2 = distance2(space, p, positions[j], self.contract);
                    if d2 <= r2 {
                        out.push((d2, ids[j], j));
                    }
                }
            }
        }
        out.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    }
}

/// Squared distance, along the shorter way round where the space wraps.
pub fn distance2(space: &Space, a: [f32; 2], b: [f32; 2], contract: bool) -> f32 {
    let d = |u: f32, v: f32, size: f32| {
        let d = (u - v).abs();
        if space.wrap { d.min(size - d) } else { d }
    };
    let (dx, dy) = (d(a[0], b[0], space.w), d(a[1], b[1], space.h));
    if contract { crate::contract::distance2(dx, dy) } else { dx * dx + dy * dy }
}
