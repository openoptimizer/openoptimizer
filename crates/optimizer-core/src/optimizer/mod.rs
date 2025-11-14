use crate::types::*;
use std::cmp::Ordering;

mod layout;
mod optional;
mod summary;
#[cfg(test)]
mod tests;

/// Packs rectangular items on panels using a best-fit decreasing heuristic.
pub struct Optimizer {
    request: OptimizationRequest,
}

impl Optimizer {
    /// Validates requests and builds a new optimizer instance.
    pub fn new(request: OptimizationRequest) -> Result<Self> {
        if request.panel_types.is_empty() {
            return Err(OptimizerError::InvalidInput(
                "At least one panel type must be provided".to_string(),
            ));
        }

        if request.items.is_empty() {
            return Err(OptimizerError::InvalidInput(
                "At least one item must be provided".to_string(),
            ));
        }

        Ok(Self { request })
    }

    /// Executes the full optimization flow and returns packed layouts.
    pub fn optimize(&self) -> Result<OptimizationResult> {
        let mut expanded_items = self.expand_items();
        expanded_items.sort_by(|a, b| {
            let area_a = a.width * a.height;
            let area_b = b.width * b.height;
            area_b.partial_cmp(&area_a).unwrap_or(Ordering::Equal)
        });

        let layouts = self.best_fit_decreasing_optimize(&expanded_items)?;
        let (final_layouts, optional_items_used) = self.try_add_optional_items(layouts)?;
        let panels_required = self.count_panels(&final_layouts);
        let summary = self.calculate_summary(&final_layouts);

        Ok(OptimizationResult {
            panels_required,
            layouts: final_layouts,
            summary,
            optional_items_used,
        })
    }

    /// Duplicates items according to their requested quantity.
    fn expand_items(&self) -> Vec<Item> {
        let mut expanded = Vec::new();
        for item in &self.request.items {
            for i in 0..item.quantity {
                expanded.push(Item {
                    id: if item.quantity > 1 {
                        format!("{}_{}", item.id, i + 1)
                    } else {
                        item.id.clone()
                    },
                    width: item.width,
                    height: item.height,
                    quantity: 1,
                    can_rotate: item.can_rotate,
                });
            }
        }
        expanded
    }

    /// Places items using best-fit decreasing across the already opened panels.
    fn best_fit_decreasing_optimize(&self, items: &[Item]) -> Result<Vec<PanelLayout>> {
        let mut layouts = Vec::new();

        for item in items {
            let mut best_fit: Option<(usize, Placement, f64, f64)> = None;

            for (idx, layout) in layouts.iter().enumerate() {
                let candidates = self.generate_candidate_placements(item, layout);
                for (placement, base_score, area_w, area_h) in candidates {
                    let leftover = (area_w * area_h) - (placement.width * placement.height);
                    let (primary_score, secondary_score) = if self.request.min_initial_usage {
                        (placement.y * 10000.0 + placement.x, leftover)
                    } else if self.request.optimize_for_reusable_remnants {
                        (-leftover, base_score)
                    } else {
                        (leftover, placement.y * 10000.0 + placement.x)
                    };

                    match best_fit {
                        None => {
                            best_fit = Some((idx, placement, primary_score, secondary_score));
                        }
                        Some((_, _, best_primary, best_secondary)) => {
                            if primary_score < best_primary
                                || (primary_score - best_primary).abs() < f64::EPSILON
                                    && secondary_score < best_secondary
                            {
                                best_fit = Some((idx, placement, primary_score, secondary_score));
                            }
                        }
                    }
                }
            }

            if let Some((idx, placement, _, _)) = best_fit {
                layouts[idx].placements.push(placement);
            } else if let Some((panel_type, placement)) = self.place_on_new_panel(item)? {
                let panel_number = layouts
                    .iter()
                    .filter(|l| l.panel_type_id == panel_type.id)
                    .count() as u32
                    + 1;

                layouts.push(PanelLayout {
                    panel_type_id: panel_type.id.clone(),
                    panel_number,
                    width: panel_type.width,
                    height: panel_type.height,
                    placements: vec![placement],
                });
            }
        }

        Ok(layouts)
    }

    /// Enumerates every feasible placement for an item on a specific panel.
    fn generate_candidate_placements(
        &self,
        item: &Item,
        layout: &PanelLayout,
    ) -> Vec<(Placement, f64, f64, f64)> {
        let unused_areas = self.find_unused_areas(layout);
        let mut candidates = Vec::new();

        for area in unused_areas {
            if item.width <= area.width && item.height <= area.height {
                let score =
                    self.score_candidate_position(item.width, item.height, &area, layout, false);
                candidates.push((
                    Placement {
                        item_id: item.id.clone(),
                        x: area.x,
                        y: area.y,
                        width: item.width,
                        height: item.height,
                        rotated: false,
                    },
                    score,
                    area.width,
                    area.height,
                ));
            }

            if item.can_rotate && item.height <= area.width && item.width <= area.height {
                let score =
                    self.score_candidate_position(item.height, item.width, &area, layout, true);
                candidates.push((
                    Placement {
                        item_id: item.id.clone(),
                        x: area.x,
                        y: area.y,
                        width: item.height,
                        height: item.width,
                        rotated: true,
                    },
                    score,
                    area.width,
                    area.height,
                ));
            }
        }

        candidates
    }

    /// Scores where to place an item in a free area depending on the optimization mode.
    fn score_candidate_position(
        &self,
        width: f64,
        height: f64,
        area: &layout::UnusedArea,
        layout: &PanelLayout,
        rotated: bool,
    ) -> f64 {
        let base_score = if self.request.min_initial_usage {
            let dist_from_origin = area.y * 1000.0 + area.x;
            let mut min_dist_to_item = f64::MAX;

            for placement in &layout.placements {
                let right_edge = placement.x + placement.width;
                let bottom_edge = placement.y + placement.height;

                if area.y < bottom_edge + 1.0 && area.y + height > placement.y - 1.0 {
                    let horiz_dist = if area.x >= right_edge {
                        area.x - right_edge
                    } else if area.x + width <= placement.x {
                        placement.x - (area.x + width)
                    } else {
                        0.0
                    };
                    min_dist_to_item = min_dist_to_item.min(horiz_dist);
                }
            }

            let mut score = if min_dist_to_item < f64::MAX {
                dist_from_origin + min_dist_to_item * 0.1
            } else {
                dist_from_origin
            };

            if rotated && height > width {
                score += (height - width) * 100.0;
            }

            score
        } else {
            area.y * 10000.0 + area.x
        };

        if self.request.optimize_for_reusable_remnants {
            let remnant_width = area.width - width;
            let remnant_height = area.height - height;
            let max_remnant_area = remnant_width * remnant_height;
            base_score - (max_remnant_area * 0.01)
        } else {
            base_score
        }
    }

    /// Opens a new panel when the item cannot be placed on existing layouts.
    fn place_on_new_panel(&self, item: &Item) -> Result<Option<(PanelType, Placement)>> {
        for panel_type in &self.request.panel_types {
            if item.width <= panel_type.width && item.height <= panel_type.height {
                return Ok(Some((
                    panel_type.clone(),
                    Placement {
                        item_id: item.id.clone(),
                        x: 0.0,
                        y: 0.0,
                        width: item.width,
                        height: item.height,
                        rotated: false,
                    },
                )));
            }

            if item.can_rotate && item.height <= panel_type.width && item.width <= panel_type.height
            {
                return Ok(Some((
                    panel_type.clone(),
                    Placement {
                        item_id: item.id.clone(),
                        x: 0.0,
                        y: 0.0,
                        width: item.height,
                        height: item.width,
                        rotated: true,
                    },
                )));
            }
        }

        Err(OptimizerError::CannotFitAll)
    }
}
