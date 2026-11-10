//! Single-panel packers. Every packer produces guillotine layouts: each placement is created by
//! cutting a free rectangle into the piece and at most two remaining rectangles.

use super::model::*;

/// Available pieces grouped by kind (identical dimensions and rotation rule).
#[derive(Debug, Clone)]
pub struct Avail {
    pub by_kind: Vec<Vec<usize>>,
    pub count: usize,
}

impl Avail {
    pub fn new(pieces: &[Piece], kinds: usize, indices: impl IntoIterator<Item = usize>) -> Self {
        let mut by_kind = vec![Vec::new(); kinds];
        let mut count = 0;
        for idx in indices {
            by_kind[pieces[idx].kind].push(idx);
            count += 1;
        }
        // Pop from the end, keep the natural order of identical pieces.
        for list in &mut by_kind {
            list.reverse();
        }
        Self { by_kind, count }
    }

    pub fn take(&mut self, kind: usize) -> Option<usize> {
        let idx = self.by_kind[kind].pop();
        if idx.is_some() {
            self.count -= 1;
        }
        idx
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// How a free rectangle is split after a piece is put into its corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitRule {
    /// Keep the larger of the two remaining rectangles as large as possible (good remnants).
    MaxArea,
    /// Split along the shorter leftover axis.
    ShorterLeftover,
}

/// Packing strategy for one panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Strips across the panel width (first-stage cuts parallel to the x axis).
    StripsAlongX,
    /// Strips along the panel height (first-stage cuts parallel to the y axis).
    StripsAlongY,
    /// Free-rectangle best fit on the whole panel.
    Fill(SplitRule),
}

pub const METHODS: [Method; 4] = [
    Method::StripsAlongX,
    Method::StripsAlongY,
    Method::Fill(SplitRule::MaxArea),
    Method::Fill(SplitRule::ShorterLeftover),
];

/// Best strip found so far: (score, strip height, chosen kinds with orientation).
type StripChoice = (f64, i64, Vec<(usize, Orientation)>);

/// Tuning knobs varied between search iterations.
#[derive(Debug, Clone)]
pub struct PackParams {
    /// Number of strip heights evaluated per strip.
    pub strip_candidates: usize,
    /// Multiplicative value noise per piece kind (1.0 = no noise).
    pub kind_noise: Vec<f64>,
    /// Number of strip alternatives completed to the panel end before a strip is chosen.
    pub lookahead: usize,
}

pub struct Packer<'a> {
    pub pieces: &'a [Piece],
    pub params: &'a PackParams,
}

impl<'a> Packer<'a> {
    /// Packs as much as possible of `avail` on one panel. Taken pieces are removed from `avail`.
    pub fn pack(&self, spec: &BinSpec, method: Method, avail: &mut Avail) -> Vec<Placed> {
        let full = Rect {
            x: 0,
            y: 0,
            w: spec.w,
            h: spec.h,
        };
        match method {
            Method::StripsAlongX => self.strips(full, false, avail),
            Method::StripsAlongY => self
                .strips(full.transposed(), true, avail)
                .into_iter()
                .map(|p| p.transposed())
                .collect(),
            Method::Fill(rule) => self.fill(vec![full], false, rule, avail),
        }
    }

    /// Best orientation of a kind for a strip of height `strip_h` and length `len`
    /// (tallest orientation that still fits, which uses the least strip length).
    fn strip_orientation(
        &self,
        kind_piece: &Piece,
        strip_h: i64,
        len: i64,
        transposed: bool,
    ) -> Option<Orientation> {
        kind_piece
            .orientations(transposed)
            .filter(|o| o.h <= strip_h && o.w <= len)
            .max_by(|a, b| a.h.cmp(&b.h).then(b.w.cmp(&a.w)))
    }

    /// Stacks strips along the frame's y axis. Each strip is filled by a bounded knapsack over
    /// the piece kinds; the space above shorter pieces and at the strip end is filled afterwards.
    fn strips(&self, frame: Rect, transposed: bool, avail: &mut Avail) -> Vec<Placed> {
        self.strips_from(0, frame, transposed, avail, self.params.lookahead)
    }

    /// Strip packing from depth `y` on. With `lookahead > 1` the best strips (by own fill
    /// rate) are each completed greedily and the strip leading to the fullest panel wins.
    fn strips_from(
        &self,
        mut y: i64,
        frame: Rect,
        transposed: bool,
        avail: &mut Avail,
        lookahead: usize,
    ) -> Vec<Placed> {
        let mut placed = Vec::new();
        loop {
            let mut candidates = self.strip_candidates(frame.h - y, frame.w, transposed, avail);
            if candidates.is_empty() {
                break;
            }
            let pick = if lookahead > 1 && candidates.len() > 1 {
                let mut best = (i64::MIN, 0);
                for (i, choice) in candidates.iter().take(lookahead).enumerate() {
                    let mut trial = avail.clone();
                    let mut used: i64 = self
                        .apply_strip(choice, y, frame.w, transposed, &mut trial)
                        .iter()
                        .map(|p| self.pieces[p.piece].real_area)
                        .sum();
                    used += self
                        .strips_from(y + choice.1, frame, transposed, &mut trial, 1)
                        .iter()
                        .map(|p| self.pieces[p.piece].real_area)
                        .sum::<i64>();
                    if used > best.0 {
                        best = (used, i);
                    }
                }
                best.1
            } else {
                0
            };
            let choice = candidates.swap_remove(pick);
            placed.extend(self.apply_strip(&choice, y, frame.w, transposed, avail));
            y += choice.1;
            if avail.is_empty() {
                break;
            }
        }
        if y < frame.h && !avail.is_empty() {
            let rest = Rect {
                x: 0,
                y,
                w: frame.w,
                h: frame.h - y,
            };
            placed.extend(self.fill(vec![rest], transposed, SplitRule::MaxArea, avail));
        }
        placed
    }

    /// Candidate strips for the remaining depth, best fill rate first.
    fn strip_candidates(
        &self,
        remaining_depth: i64,
        len: i64,
        transposed: bool,
        avail: &Avail,
    ) -> Vec<StripChoice> {
        let mut heights: Vec<i64> = Vec::new();
        for list in &avail.by_kind {
            if let Some(&idx) = list.last() {
                for o in self.pieces[idx].orientations(transposed) {
                    if o.h <= remaining_depth && o.w <= len {
                        heights.push(o.h);
                    }
                }
            }
        }
        heights.sort_unstable_by(|a, b| b.cmp(a));
        heights.dedup();
        heights.truncate(self.params.strip_candidates.max(1));
        let mut candidates: Vec<StripChoice> = heights
            .into_iter()
            .filter_map(|strip_h| {
                let (value, chosen) = self.knapsack(strip_h, len, transposed, avail);
                (!chosen.is_empty())
                    .then(|| (value / (len as f64 * strip_h as f64), strip_h, chosen))
            })
            .collect();
        candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        candidates
    }

    /// Places one strip at depth `y` and fills the space above shorter pieces and at its end.
    fn apply_strip(
        &self,
        choice: &StripChoice,
        y: i64,
        len: i64,
        transposed: bool,
        avail: &mut Avail,
    ) -> Vec<Placed> {
        let strip_h = choice.1;
        let mut chosen = choice.2.clone();
        // Tallest first: the free space above the pieces becomes a staircase that merges well.
        chosen.sort_by(|a, b| b.1.h.cmp(&a.1.h).then(b.1.w.cmp(&a.1.w)));
        let mut placed = Vec::with_capacity(chosen.len());
        let mut x = 0;
        let mut free: Vec<Rect> = Vec::new();
        for (kind, o) in chosen {
            let idx = avail
                .take(kind)
                .expect("knapsack only uses available kinds");
            placed.push(Placed {
                piece: idx,
                x,
                y,
                w: o.w,
                h: o.h,
                rotated: o.rotated,
            });
            if o.h < strip_h {
                let top = Rect {
                    x,
                    y: y + o.h,
                    w: o.w,
                    h: strip_h - o.h,
                };
                match free.last_mut() {
                    Some(last) if last.y == top.y && last.x + last.w == top.x => last.w += top.w,
                    _ => free.push(top),
                }
            }
            x += o.w;
        }
        if x < len {
            free.push(Rect {
                x,
                y,
                w: len - x,
                h: strip_h,
            });
        }
        placed.extend(self.fill(free, transposed, SplitRule::MaxArea, avail));
        placed
    }

    /// Bounded knapsack over kinds: maximise covered (inflated) area of a strip.
    /// Widths are rounded up to whole millimetres so the chosen set always fits exactly.
    fn knapsack(
        &self,
        strip_h: i64,
        len: i64,
        transposed: bool,
        avail: &Avail,
    ) -> (f64, Vec<(usize, Orientation)>) {
        const UNIT: i64 = 100;
        let cap = (len / UNIT) as usize;
        // (kind, orientation, copies, weight, value)
        let mut items: Vec<(usize, Orientation, usize, usize, f64)> = Vec::new();
        for (kind, list) in avail.by_kind.iter().enumerate() {
            let Some(&idx) = list.last() else { continue };
            let Some(o) = self.strip_orientation(&self.pieces[idx], strip_h, len, transposed)
            else {
                continue;
            };
            let weight = ((o.w + UNIT - 1) / UNIT) as usize;
            if weight == 0 || weight > cap {
                continue;
            }
            let value = (o.w as f64) * (o.h as f64) * self.params.kind_noise[kind];
            let max_copies = list.len().min(cap / weight);
            // Binary splitting of bounded copies.
            let mut remaining = max_copies;
            let mut chunk = 1;
            while remaining > 0 {
                let take = chunk.min(remaining);
                items.push((kind, o, take, weight * take, value * take as f64));
                remaining -= take;
                chunk *= 2;
            }
        }
        if items.is_empty() {
            return (0.0, Vec::new());
        }
        let width = cap + 1;
        let mut best = vec![0.0f64; width];
        let mut keep = vec![false; items.len() * width];
        for (i, &(_, _, _, weight, value)) in items.iter().enumerate() {
            for c in (weight..=cap).rev() {
                let candidate = best[c - weight] + value;
                if candidate > best[c] + 1e-9 {
                    best[c] = candidate;
                    keep[i * width + c] = true;
                }
            }
        }
        let mut c = cap;
        let mut chosen = Vec::new();
        for i in (0..items.len()).rev() {
            if keep[i * width + c] {
                let (kind, o, copies, weight, _) = items[i];
                for _ in 0..copies {
                    chosen.push((kind, o));
                }
                c -= weight;
            }
        }
        (best[cap], chosen)
    }

    /// Guillotine free-rectangle best fit: repeatedly takes the largest free rectangle and puts
    /// the largest fitting piece into its corner.
    pub fn fill(
        &self,
        mut free: Vec<Rect>,
        transposed: bool,
        rule: SplitRule,
        avail: &mut Avail,
    ) -> Vec<Placed> {
        let mut placed = Vec::new();
        let (min_w, min_h) = self.min_dims(avail, transposed);
        free.retain(|r| r.w >= min_w && r.h >= min_h);
        while !avail.is_empty() {
            let Some(pos) = free
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.area().cmp(&b.1.area()).then(b.0.cmp(&a.0)))
                .map(|(i, _)| i)
            else {
                break;
            };
            let r = free.swap_remove(pos);
            let mut best: Option<(f64, i64, usize, Orientation)> = None;
            for (kind, list) in avail.by_kind.iter().enumerate() {
                let Some(&idx) = list.last() else { continue };
                for o in self.pieces[idx].orientations(transposed) {
                    if o.w > r.w || o.h > r.h {
                        continue;
                    }
                    let value = (o.w as f64) * (o.h as f64) * self.params.kind_noise[kind];
                    let short = (r.w - o.w).min(r.h - o.h);
                    let better = match &best {
                        None => true,
                        Some((v, s, _, _)) => {
                            value > *v + 1e-9 || (value >= *v - 1e-9 && short < *s)
                        }
                    };
                    if better {
                        best = Some((value, short, kind, o));
                    }
                }
            }
            let Some((_, _, kind, o)) = best else {
                continue;
            };
            let idx = avail.take(kind).expect("kind is available");
            placed.push(Placed {
                piece: idx,
                x: r.x,
                y: r.y,
                w: o.w,
                h: o.h,
                rotated: o.rotated,
            });
            let (a, b) = split(r, o.w, o.h, rule);
            for rect in [a, b].into_iter().flatten() {
                if rect.w >= min_w && rect.h >= min_h {
                    free.push(rect);
                }
            }
        }
        placed
    }

    /// Smallest width/height any available piece can have (for pruning useless rectangles).
    fn min_dims(&self, avail: &Avail, transposed: bool) -> (i64, i64) {
        let mut min_w = i64::MAX;
        let mut min_h = i64::MAX;
        for list in &avail.by_kind {
            if let Some(&idx) = list.last() {
                for o in self.pieces[idx].orientations(transposed) {
                    min_w = min_w.min(o.w);
                    min_h = min_h.min(o.h);
                }
            }
        }
        (min_w, min_h)
    }
}

/// Splits a free rectangle after a `w`×`h` piece was put into its lower-left corner.
/// Returns the right and the top remainder (either may be absent).
pub fn split(r: Rect, w: i64, h: i64, rule: SplitRule) -> (Option<Rect>, Option<Rect>) {
    let right_w = r.w - w;
    let top_h = r.h - h;
    // Option A: horizontal cut first -> top spans the full width.
    let a = (
        Rect {
            x: r.x + w,
            y: r.y,
            w: right_w,
            h,
        },
        Rect {
            x: r.x,
            y: r.y + h,
            w: r.w,
            h: top_h,
        },
    );
    // Option B: vertical cut first -> right spans the full height.
    let b = (
        Rect {
            x: r.x + w,
            y: r.y,
            w: right_w,
            h: r.h,
        },
        Rect {
            x: r.x,
            y: r.y + h,
            w,
            h: top_h,
        },
    );
    let use_a = match rule {
        SplitRule::MaxArea => a.1.area().max(a.0.area()) >= b.0.area().max(b.1.area()),
        SplitRule::ShorterLeftover => right_w < top_h,
    };
    let (right, top) = if use_a { a } else { b };
    let keep = |r: Rect| (r.w > 0 && r.h > 0).then_some(r);
    (keep(right), keep(top))
}
