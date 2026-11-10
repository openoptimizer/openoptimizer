//! Recovers the guillotine cut tree of a packed panel: the ordered saw cuts and the free
//! rectangles (offcuts) that remain. Works on inflated coordinates (see `model`).

use super::model::*;

/// A straight cut through a whole region. `pos` is the inflated boundary, the kerf occupies
/// `[pos - kerf, pos)` in real coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CutLine {
    pub vertical: bool,
    pub pos: i64,
    pub from: i64,
    pub to: i64,
    pub stage: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Decomposition {
    pub cuts: Vec<CutLine>,
    /// Free leaves of the cut tree (inflated), disjoint.
    pub free: Vec<Rect>,
    pub stages: u32,
}

/// Decomposes a panel. Returns `None` if the placements cannot be produced with guillotine cuts.
pub fn decompose(placed: &[Placed], region: Rect) -> Option<Decomposition> {
    let mut out = Decomposition::default();
    let rects: Vec<Rect> = placed.iter().map(|p| p.rect()).collect();
    recurse(&rects, region, 1, None, &mut out)?;
    Some(out)
}

/// Splits `region` along one axis into slices that each contain whole pieces.
/// Returns the slice boundaries (exclusive of region start, inclusive of internal cuts only).
fn slices(rects: &[Rect], region: Rect, vertical: bool) -> Vec<i64> {
    let lo = |r: &Rect| if vertical { r.x } else { r.y };
    let hi = |r: &Rect| if vertical { r.x + r.w } else { r.y + r.h };
    let (start, end) = if vertical {
        (region.x, region.x + region.w)
    } else {
        (region.y, region.y + region.h)
    };
    let mut sorted: Vec<&Rect> = rects.iter().collect();
    sorted.sort_by_key(|r| lo(r));
    let mut bounds = Vec::new();
    let mut reach = start;
    let mut first = true;
    for r in sorted {
        if lo(r) >= reach && !first {
            bounds.push(reach);
        } else if first && lo(r) > start {
            // Free space before the first piece becomes its own slice.
            bounds.push(lo(r));
        }
        first = false;
        reach = reach.max(hi(r));
    }
    if reach < end {
        bounds.push(reach);
    }
    bounds
}

fn recurse(
    rects: &[Rect],
    region: Rect,
    stage: u32,
    parent_vertical: Option<bool>,
    out: &mut Decomposition,
) -> Option<()> {
    if rects.is_empty() {
        if region.w > 0 && region.h > 0 {
            out.free.push(region);
        }
        return Some(());
    }
    if rects.len() == 1 && rects[0] == region {
        out.stages = out.stages.max(stage - 1);
        return Some(());
    }
    // Prefer alternating cut directions (fewer stages); on the first stage prefer the direction
    // that leaves the larger trailing offcut.
    let order = match parent_vertical {
        Some(v) => [!v, v],
        None => {
            let tail = |vertical: bool| -> i64 {
                let b = slices(rects, region, vertical);
                match b.last() {
                    Some(&last) if vertical => (region.x + region.w - last) * region.h,
                    Some(&last) => (region.y + region.h - last) * region.w,
                    None => 0,
                }
            };
            if tail(true) >= tail(false) {
                [true, false]
            } else {
                [false, true]
            }
        }
    };
    for vertical in order {
        let bounds = slices(rects, region, vertical);
        if bounds.is_empty() {
            continue;
        }
        let (start, end) = if vertical {
            (region.x, region.x + region.w)
        } else {
            (region.y, region.y + region.h)
        };
        let mut edges = vec![start];
        edges.extend(bounds.iter().copied());
        edges.push(end);
        for &b in &bounds {
            out.cuts.push(CutLine {
                vertical,
                pos: b,
                from: if vertical { region.y } else { region.x },
                to: if vertical {
                    region.y + region.h
                } else {
                    region.x + region.w
                },
                stage,
            });
        }
        out.stages = out.stages.max(stage);
        for pair in edges.windows(2) {
            let sub = if vertical {
                Rect {
                    x: pair[0],
                    y: region.y,
                    w: pair[1] - pair[0],
                    h: region.h,
                }
            } else {
                Rect {
                    x: region.x,
                    y: pair[0],
                    w: region.w,
                    h: pair[1] - pair[0],
                }
            };
            let inside: Vec<Rect> = rects
                .iter()
                .copied()
                .filter(|r| {
                    let (l, h) = if vertical {
                        (r.x, r.x + r.w)
                    } else {
                        (r.y, r.y + r.h)
                    };
                    l >= pair[0] && h <= pair[1]
                })
                .collect();
            recurse(&inside, sub, stage + 1, Some(vertical), out)?;
        }
        return Some(());
    }
    None
}
