use super::guillotine::decompose;
use super::model::*;
use super::packer::{split, SplitRule};
use super::*;

/// Minimum effective waste percentage required before optional items are considered.
pub(super) const MIN_WASTE_THRESHOLD_PERCENT: f64 = 8.0;

#[derive(Debug, Clone)]
pub(super) struct OptionalPlacement {
    pub id: String,
    pub placed: Placed,
}

impl Optimizer {
    /// Fills the offcuts of every panel with optional items of its panel type.
    /// Items are tried by descending priority (then area); an item may be used several times.
    /// Optional items never open additional panels and keep the plan guillotine-cuttable.
    pub(super) fn place_optional_items(&self, bins: &[PackedBin]) -> Vec<Vec<OptionalPlacement>> {
        let kerf = self.kerf();
        bins.iter()
            .map(|bin| {
                let panel = &self.request.panel_types[bin.spec.type_index];
                if panel.optional_items.is_empty() {
                    return Vec::new();
                }
                let mut options: Vec<&OptionalItem> = panel
                    .optional_items
                    .iter()
                    .filter(|o| o.width > 0.0 && o.height > 0.0)
                    .collect();
                options.sort_by(|a, b| {
                    b.priority.cmp(&a.priority).then_with(|| {
                        (b.width * b.height)
                            .partial_cmp(&(a.width * a.height))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                });
                let region = Rect {
                    x: 0,
                    y: 0,
                    w: bin.spec.w,
                    h: bin.spec.h,
                };
                let Some(decomposition) = decompose(&bin.placed, region) else {
                    return Vec::new();
                };
                let mut free = decomposition.free;
                let mut placed = Vec::new();
                while let Some(pos) = free
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, r)| r.area())
                    .map(|(i, _)| i)
                {
                    let r = free.swap_remove(pos);
                    let fit = options.iter().find_map(|o| {
                        let w = to_units(o.width) + kerf;
                        let h = to_units(o.height) + kerf;
                        if w <= r.w && h <= r.h {
                            Some((o, w, h, false))
                        } else if o.can_rotate && h <= r.w && w <= r.h {
                            Some((o, h, w, true))
                        } else {
                            None
                        }
                    });
                    let Some((item, w, h, rotated)) = fit else {
                        continue;
                    };
                    placed.push(OptionalPlacement {
                        id: item.id.clone(),
                        placed: Placed {
                            piece: usize::MAX,
                            x: r.x,
                            y: r.y,
                            w,
                            h,
                            rotated,
                        },
                    });
                    let (a, b) = split(r, w, h, SplitRule::MaxArea);
                    free.extend([a, b].into_iter().flatten());
                }
                placed
            })
            .collect()
    }
}
