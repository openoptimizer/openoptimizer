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

        for panel in &request.panel_types {
            if panel.trimming < 0.0 {
                return Err(OptimizerError::InvalidInput(format!(
                    "Panel '{}' has negative trimming",
                    panel.id
                )));
            }

            let usable_width = panel.width - (panel.trimming * 2.0);
            let usable_height = panel.height - (panel.trimming * 2.0);

            if usable_width <= 0.0 || usable_height <= 0.0 {
                return Err(OptimizerError::InvalidInput(format!(
                    "Panel '{}' becomes unusable after applying trimming",
                    panel.id
                )));
            }
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
        let (mut final_layouts, optional_items_used) = self.try_add_optional_items(layouts)?;

        // Compute unused areas for each panel in the final output
        for layout in &mut final_layouts {
            layout.unused_areas = self.compute_output_unused_areas(layout);
        }

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

    /// Places items using best-fit decreasing with bottom-left placement strategy.
    /// Items are placed as far left and down as possible to minimize fragmentation.
    fn best_fit_decreasing_optimize(&self, items: &[Item]) -> Result<Vec<PanelLayout>> {
        let mut layouts = Vec::new();

        for item in items {
            let mut best_fit: Option<(usize, Placement, f64)> = None;

            // Try to place on existing panels using bottom-left-fill strategy
            for (idx, layout) in layouts.iter().enumerate() {
                if let Some((placement, score)) = self.find_best_placement(item, layout) {
                    match best_fit {
                        None => {
                            best_fit = Some((idx, placement, score));
                        }
                        Some((_, _, best_score)) => {
                            if score < best_score {
                                best_fit = Some((idx, placement, score));
                            }
                        }
                    }
                }
            }

            if let Some((idx, placement, _)) = best_fit {
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
                    trimming: panel_type.trimming,
                    placements: vec![placement],
                    unused_areas: Vec::new(), // Populated after optimization completes
                });
            }
        }

        Ok(layouts)
    }

    /// Finds the best placement position for an item on a panel using bottom-left-fill.
    /// Returns the placement and a score (lower is better).
    fn find_best_placement(&self, item: &Item, layout: &PanelLayout) -> Option<(Placement, f64)> {
        let unused_areas = self.find_unused_areas(layout);
        let mut best: Option<(Placement, f64)> = None;

        for area in &unused_areas {
            // Try normal orientation
            if item.width <= area.width && item.height <= area.height {
                let score = self.calculate_placement_score(
                    area.x,
                    area.y,
                    item.width,
                    item.height,
                    area,
                    layout,
                );
                let placement = Placement {
                    item_id: item.id.clone(),
                    x: area.x,
                    y: area.y,
                    width: item.width,
                    height: item.height,
                    rotated: false,
                };

                match best {
                    None => best = Some((placement, score)),
                    Some((_, best_score)) if score < best_score => {
                        best = Some((placement, score));
                    }
                    _ => {}
                }
            }

            // Try rotated orientation
            if item.can_rotate && item.height <= area.width && item.width <= area.height {
                let score = self.calculate_placement_score(
                    area.x,
                    area.y,
                    item.height,
                    item.width,
                    area,
                    layout,
                );
                let placement = Placement {
                    item_id: item.id.clone(),
                    x: area.x,
                    y: area.y,
                    width: item.height,
                    height: item.width,
                    rotated: true,
                };

                match best {
                    None => best = Some((placement, score)),
                    Some((_, best_score)) if score < best_score => {
                        best = Some((placement, score));
                    }
                    _ => {}
                }
            }
        }

        best
    }

    /// Calculates a placement score for bottom-left-fill strategy.
    /// Lower score = better placement. Prioritizes:
    /// 1. Bottom-left position (y first, then x)
    /// 2. Tight fit (less wasted space in the free rectangle)
    /// 3. Contact with existing pieces (better packing)
    fn calculate_placement_score(
        &self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        area: &layout::UnusedArea,
        layout: &PanelLayout,
    ) -> f64 {
        // Primary: Bottom-left position - items should fill from bottom-left
        // Heavily weight Y to ensure rows are filled before going up
        let position_score = y * 10000.0 + x;

        // Secondary: How well does the item fit the free rectangle?
        // Prefer placements that leave useful remaining space
        let width_leftover = area.width - width - self.request.cut_width;
        let height_leftover = area.height - height - self.request.cut_width;

        // Penalty for creating thin slivers that are hard to use
        let sliver_penalty = if width_leftover > 0.0 && width_leftover < 50.0 {
            (50.0 - width_leftover) * 10.0
        } else {
            0.0
        } + if height_leftover > 0.0 && height_leftover < 50.0 {
            (50.0 - height_leftover) * 10.0
        } else {
            0.0
        };

        // Tertiary: Contact score - prefer placements adjacent to existing pieces
        let contact_score = self.calculate_contact_score(x, y, width, height, layout);

        // Combine scores: position is most important, then contact, then sliver avoidance
        if self.request.min_initial_usage {
            // Pack tightly from origin
            position_score - contact_score * 100.0 + sliver_penalty
        } else if self.request.optimize_for_reusable_remnants {
            // Prefer leaving large contiguous areas
            position_score + sliver_penalty * 2.0 - contact_score * 50.0
        } else {
            // Default: bottom-left with contact bonus
            position_score - contact_score * 200.0 + sliver_penalty
        }
    }

    /// Calculates how much contact this placement has with existing pieces or edges.
    /// Higher contact score = better (placement touches more edges/pieces).
    fn calculate_contact_score(
        &self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        layout: &PanelLayout,
    ) -> f64 {
        let mut contact = 0.0;
        let eps = 1.0;

        // Contact with panel edges (trimming boundary)
        if (x - layout.trimming).abs() < eps {
            contact += height; // Left edge contact
        }
        if (y - layout.trimming).abs() < eps {
            contact += width; // Bottom edge contact
        }

        let right_boundary = layout.width - layout.trimming;
        let top_boundary = layout.height - layout.trimming;

        if (x + width - right_boundary).abs() < eps {
            contact += height; // Right edge contact
        }
        if (y + height - top_boundary).abs() < eps {
            contact += width; // Top edge contact
        }

        // Contact with existing placements
        for placement in &layout.placements {
            let p_right = placement.x + placement.width;
            let p_top = placement.y + placement.height;

            // Check for horizontal adjacency (left/right contact)
            let v_overlap_start = y.max(placement.y);
            let v_overlap_end = (y + height).min(p_top);
            if v_overlap_end > v_overlap_start {
                let v_overlap = v_overlap_end - v_overlap_start;

                // Item's left edge touches placement's right edge
                if (x - p_right - self.request.cut_width).abs() < eps {
                    contact += v_overlap;
                }
                // Item's right edge touches placement's left edge
                if (x + width + self.request.cut_width - placement.x).abs() < eps {
                    contact += v_overlap;
                }
            }

            // Check for vertical adjacency (top/bottom contact)
            let h_overlap_start = x.max(placement.x);
            let h_overlap_end = (x + width).min(p_right);
            if h_overlap_end > h_overlap_start {
                let h_overlap = h_overlap_end - h_overlap_start;

                // Item's bottom edge touches placement's top edge
                if (y - p_top - self.request.cut_width).abs() < eps {
                    contact += h_overlap;
                }
                // Item's top edge touches placement's bottom edge
                if (y + height + self.request.cut_width - placement.y).abs() < eps {
                    contact += h_overlap;
                }
            }
        }

        contact
    }

    /// Enumerates every feasible placement for an item on a specific panel.
    /// Used by optional item placement.
    fn generate_candidate_placements(
        &self,
        item: &Item,
        layout: &PanelLayout,
    ) -> Vec<(Placement, f64, f64, f64)> {
        let unused_areas = self.find_unused_areas(layout);
        let mut candidates = Vec::new();

        for area in unused_areas {
            if item.width <= area.width && item.height <= area.height {
                let score = self.calculate_placement_score(
                    area.x,
                    area.y,
                    item.width,
                    item.height,
                    &area,
                    layout,
                );
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
                let score = self.calculate_placement_score(
                    area.x,
                    area.y,
                    item.height,
                    item.width,
                    &area,
                    layout,
                );
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

    /// Opens a new panel when the item cannot be placed on existing layouts.
    fn place_on_new_panel(&self, item: &Item) -> Result<Option<(PanelType, Placement)>> {
        for panel_type in &self.request.panel_types {
            let usable_width = panel_type.width - (panel_type.trimming * 2.0);
            let usable_height = panel_type.height - (panel_type.trimming * 2.0);

            if item.width <= usable_width && item.height <= usable_height {
                return Ok(Some((
                    panel_type.clone(),
                    Placement {
                        item_id: item.id.clone(),
                        x: panel_type.trimming,
                        y: panel_type.trimming,
                        width: item.width,
                        height: item.height,
                        rotated: false,
                    },
                )));
            }

            if item.can_rotate && item.height <= usable_width && item.width <= usable_height {
                return Ok(Some((
                    panel_type.clone(),
                    Placement {
                        item_id: item.id.clone(),
                        x: panel_type.trimming,
                        y: panel_type.trimming,
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
