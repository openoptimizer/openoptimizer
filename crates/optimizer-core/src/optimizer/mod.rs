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
    ///
    /// Runs the BFD heuristic with several sort/rotation strategies, keeps the
    /// result that uses the fewest panels (ties broken by lowest waste), then
    /// attempts to consolidate by redistributing items from the least-used
    /// panel into the remaining ones.
    pub fn optimize(&self) -> Result<OptimizationResult> {
        let expanded_items = self.expand_items();
        let strategies = self.generate_sort_strategies(&expanded_items);

        let mut best_layouts: Option<Vec<PanelLayout>> = None;
        let mut best_panel_count = usize::MAX;
        let mut best_waste = f64::MAX;

        for sorted_items in &strategies {
            match self.best_fit_decreasing_optimize(sorted_items) {
                Ok(layouts) => {
                    let layouts = self.try_reduce_panels(layouts, &expanded_items);
                    let panel_count = layouts.len();
                    let summary = self.calculate_summary(&layouts);
                    let waste = summary.waste_area;

                    if panel_count < best_panel_count
                        || (panel_count == best_panel_count && waste < best_waste)
                    {
                        best_layouts = Some(layouts);
                        best_panel_count = panel_count;
                        best_waste = waste;
                    }
                }
                Err(_) => continue,
            }
        }

        let layouts = best_layouts.ok_or(OptimizerError::CannotFitAll)?;
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

    /// Builds several item orderings (and optional dimension pre-normalizations)
    /// so the BFD heuristic can explore different packing configurations.
    fn generate_sort_strategies(&self, items: &[Item]) -> Vec<Vec<Item>> {
        let mut strategies: Vec<Vec<Item>> = Vec::new();

        // Helper: normalize items so height = max dimension (only for rotatable items)
        let normalize_tall = |items: &[Item]| -> Vec<Item> {
            items
                .iter()
                .map(|item| {
                    if item.can_rotate && item.width > item.height {
                        Item {
                            id: item.id.clone(),
                            width: item.height,
                            height: item.width,
                            quantity: item.quantity,
                            can_rotate: item.can_rotate,
                        }
                    } else {
                        item.clone()
                    }
                })
                .collect()
        };

        // Helper: normalize items so width = max dimension (only for rotatable items)
        let normalize_wide = |items: &[Item]| -> Vec<Item> {
            items
                .iter()
                .map(|item| {
                    if item.can_rotate && item.height > item.width {
                        Item {
                            id: item.id.clone(),
                            width: item.height,
                            height: item.width,
                            quantity: item.quantity,
                            can_rotate: item.can_rotate,
                        }
                    } else {
                        item.clone()
                    }
                })
                .collect()
        };

        // Strategy 1: area descending (standard BFD)
        let mut s = items.to_vec();
        s.sort_by(|a, b| {
            let area_a = a.width * a.height;
            let area_b = b.width * b.height;
            area_b.partial_cmp(&area_a).unwrap_or(Ordering::Equal)
        });
        strategies.push(s);

        // Strategy 2: height descending, then width descending
        let mut s = items.to_vec();
        s.sort_by(
            |a, b| match b.height.partial_cmp(&a.height).unwrap_or(Ordering::Equal) {
                Ordering::Equal => b.width.partial_cmp(&a.width).unwrap_or(Ordering::Equal),
                other => other,
            },
        );
        strategies.push(s);

        // Strategy 3: width descending, then height descending
        let mut s = items.to_vec();
        s.sort_by(
            |a, b| match b.width.partial_cmp(&a.width).unwrap_or(Ordering::Equal) {
                Ordering::Equal => b.height.partial_cmp(&a.height).unwrap_or(Ordering::Equal),
                other => other,
            },
        );
        strategies.push(s);

        // Strategy 4: normalize tall, sort by max-dim descending then min-dim
        let mut s = normalize_tall(items);
        s.sort_by(
            |a, b| match b.height.partial_cmp(&a.height).unwrap_or(Ordering::Equal) {
                Ordering::Equal => b.width.partial_cmp(&a.width).unwrap_or(Ordering::Equal),
                other => other,
            },
        );
        strategies.push(s);

        // Strategy 5: normalize wide, sort by width descending then height
        let mut s = normalize_wide(items);
        s.sort_by(
            |a, b| match b.width.partial_cmp(&a.width).unwrap_or(Ordering::Equal) {
                Ordering::Equal => b.height.partial_cmp(&a.height).unwrap_or(Ordering::Equal),
                other => other,
            },
        );
        strategies.push(s);

        // Strategy 6: normalize tall, sort by area descending
        let mut s = normalize_tall(items);
        s.sort_by(|a, b| {
            let area_a = a.width * a.height;
            let area_b = b.width * b.height;
            area_b.partial_cmp(&area_a).unwrap_or(Ordering::Equal)
        });
        strategies.push(s);

        strategies
    }

    /// After the initial BFD pass, try to eliminate the least-used panel by
    /// redistributing its items across the remaining panels. Repeat until
    /// no more panels can be removed.
    fn try_reduce_panels(
        &self,
        layouts: Vec<PanelLayout>,
        expanded_items: &[Item],
    ) -> Vec<PanelLayout> {
        if layouts.len() <= 1 {
            return layouts;
        }

        let mut current = layouts;

        loop {
            if current.len() <= 1 {
                break;
            }

            // Find the panel with the smallest used area (best candidate for removal)
            let min_idx = current
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let used_a: f64 = a.placements.iter().map(|p| p.width * p.height).sum();
                    let used_b: f64 = b.placements.iter().map(|p| p.width * p.height).sum();
                    used_a.partial_cmp(&used_b).unwrap_or(Ordering::Equal)
                })
                .map(|(i, _)| i)
                .unwrap();

            let target_panel = current[min_idx].clone();

            // Reconstruct Item structs from the panel's placements
            let mut items_to_place: Vec<Item> = target_panel
                .placements
                .iter()
                .map(|p| {
                    let (orig_w, orig_h) = if p.rotated {
                        (p.height, p.width)
                    } else {
                        (p.width, p.height)
                    };

                    let can_rotate = expanded_items
                        .iter()
                        .find(|i| i.id == p.item_id)
                        .map(|i| i.can_rotate)
                        .unwrap_or(true);

                    Item {
                        id: p.item_id.clone(),
                        width: orig_w,
                        height: orig_h,
                        quantity: 1,
                        can_rotate,
                    }
                })
                .collect();

            // Place largest items first for best fit
            items_to_place.sort_by(|a, b| {
                let area_a = a.width * a.height;
                let area_b = b.width * b.height;
                area_b.partial_cmp(&area_a).unwrap_or(Ordering::Equal)
            });

            // Remove the target panel and try to redistribute its items
            let mut test = current.clone();
            test.remove(min_idx);

            let mut all_placed = true;
            for item in &items_to_place {
                let mut best_fit: Option<(usize, Placement, f64)> = None;

                for (idx, layout) in test.iter().enumerate() {
                    if let Some((placement, score)) = self.find_best_placement(item, layout) {
                        let adjusted_score = if self.request.min_initial_usage {
                            score + (idx as f64) * 1_000_000.0
                        } else {
                            score
                        };

                        match best_fit {
                            None => best_fit = Some((idx, placement, adjusted_score)),
                            Some((_, _, best_score)) if adjusted_score < best_score => {
                                best_fit = Some((idx, placement, adjusted_score));
                            }
                            _ => {}
                        }
                    }
                }

                if let Some((idx, placement, _)) = best_fit {
                    test[idx].placements.push(placement);
                } else {
                    all_placed = false;
                    break;
                }
            }

            if all_placed {
                current = test;
                // Continue and try to eliminate another panel
            } else {
                break;
            }
        }

        // Renumber panels per panel type
        self.renumber_panels(&mut current);
        current
    }

    /// Reassigns sequential panel_number values per panel_type_id.
    fn renumber_panels(&self, layouts: &mut [PanelLayout]) {
        let mut type_counts: Vec<(String, u32)> = Vec::new();
        for layout in layouts.iter_mut() {
            let num = if let Some((_, count)) = type_counts
                .iter_mut()
                .find(|(id, _)| *id == layout.panel_type_id)
            {
                *count += 1;
                *count
            } else {
                type_counts.push((layout.panel_type_id.clone(), 1));
                1
            };
            layout.panel_number = num;
        }
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
                    // When min_initial_usage is set, strongly prefer filling earlier panels
                    let adjusted_score = if self.request.min_initial_usage {
                        // Add large penalty for using later panels to encourage filling earlier ones
                        score + (idx as f64) * 1_000_000.0
                    } else {
                        score
                    };

                    match best_fit {
                        None => {
                            best_fit = Some((idx, placement, adjusted_score));
                        }
                        Some((_, _, best_score)) => {
                            if adjusted_score < best_score {
                                best_fit = Some((idx, placement, adjusted_score));
                            }
                        }
                    }
                }
            }

            if let Some((idx, placement, _)) = best_fit {
                layouts[idx].placements.push(placement);
            } else if let Some((panel_type, panel_width, panel_height, placement)) =
                self.place_on_new_panel(item)?
            {
                let panel_number = layouts
                    .iter()
                    .filter(|l| l.panel_type_id == panel_type.id)
                    .count() as u32
                    + 1;

                layouts.push(PanelLayout {
                    panel_type_id: panel_type.id.clone(),
                    panel_number,
                    width: panel_width,
                    height: panel_height,
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
        // How well does the item fit the free rectangle?
        let width_leftover = area.width - width - self.request.cut_width;
        let height_leftover = area.height - height - self.request.cut_width;

        // Calculate how tight the fit is (0 = perfect fit, higher = more waste)
        let fit_ratio = (width_leftover.max(0.0) * height_leftover.max(0.0))
            / (area.width * area.height).max(1.0);

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

        if self.request.min_initial_usage {
            // For min_initial_usage, prioritize:
            // 1. Tight fit (filling gaps perfectly) - most important
            // 2. Contact with existing pieces (pack densely)
            // 3. Bottom-left position within the panel
            let position_score = y * 100.0 + x * 0.1;

            // Bonus for tight height fit - strongly prefer filling vertical gaps
            let height_fit_bonus = if height_leftover.abs() < 10.0 {
                -50000.0 // Perfect height fit gets huge bonus
            } else if height_leftover > 0.0 && height_leftover < 100.0 {
                -20000.0 // Good height fit gets good bonus
            } else {
                0.0
            };

            // Bonus for tight width fit
            let width_fit_bonus = if width_leftover.abs() < 10.0 {
                -30000.0
            } else if width_leftover > 0.0 && width_leftover < 100.0 {
                -10000.0
            } else {
                0.0
            };

            position_score + fit_ratio * 10000.0 - contact_score * 500.0
                + sliver_penalty
                + height_fit_bonus
                + width_fit_bonus
        } else if self.request.optimize_for_reusable_remnants {
            // Prefer leaving large contiguous areas
            let position_score = y * 10000.0 + x;
            position_score + sliver_penalty * 2.0 - contact_score * 50.0
        } else {
            // Default: bottom-left with contact bonus
            let position_score = y * 10000.0 + x;
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
    fn place_on_new_panel(&self, item: &Item) -> Result<Option<(PanelType, f64, f64, Placement)>> {
        let mut best_candidate: Option<(PanelType, f64, f64, Placement, f64, u32)> = None;

        for panel_type in &self.request.panel_types {
            let orientations = if (panel_type.width - panel_type.height).abs() < f64::EPSILON {
                vec![(panel_type.width, panel_type.height)]
            } else {
                vec![
                    (panel_type.width, panel_type.height),
                    (panel_type.height, panel_type.width),
                ]
            };

            for (panel_width, panel_height) in orientations {
                if let Some((placement, score)) =
                    self.best_new_panel_placement(item, panel_type, panel_width, panel_height)
                {
                    let capacity = self.estimate_panel_capacity(
                        item,
                        panel_width,
                        panel_height,
                        panel_type.trimming,
                    );
                    match best_candidate {
                        None => {
                            best_candidate = Some((
                                panel_type.clone(),
                                panel_width,
                                panel_height,
                                placement,
                                score,
                                capacity,
                            ));
                        }
                        Some((_, _, _, _, best_score, best_capacity)) => {
                            if capacity > best_capacity
                                || (capacity == best_capacity && score < best_score)
                            {
                                best_candidate = Some((
                                    panel_type.clone(),
                                    panel_width,
                                    panel_height,
                                    placement,
                                    score,
                                    capacity,
                                ));
                            }
                        }
                    }
                }
            }
        }

        if let Some((panel_type, panel_width, panel_height, placement, _, _)) = best_candidate {
            return Ok(Some((panel_type, panel_width, panel_height, placement)));
        }

        Err(OptimizerError::CannotFitAll)
    }

    /// Picks the best placement for an item on a fresh panel orientation.
    fn best_new_panel_placement(
        &self,
        item: &Item,
        panel_type: &PanelType,
        panel_width: f64,
        panel_height: f64,
    ) -> Option<(Placement, f64)> {
        let usable_width = panel_width - (panel_type.trimming * 2.0);
        let usable_height = panel_height - (panel_type.trimming * 2.0);

        if usable_width <= 0.0 || usable_height <= 0.0 {
            return None;
        }

        let layout = PanelLayout {
            panel_type_id: panel_type.id.clone(),
            panel_number: 1,
            width: panel_width,
            height: panel_height,
            trimming: panel_type.trimming,
            placements: Vec::new(),
            unused_areas: Vec::new(),
        };

        let area = layout::UnusedArea {
            x: panel_type.trimming,
            y: panel_type.trimming,
            width: usable_width,
            height: usable_height,
        };

        let mut best: Option<(Placement, f64)> = None;

        if item.width <= usable_width && item.height <= usable_height {
            let score = self.calculate_placement_score(
                area.x,
                area.y,
                item.width,
                item.height,
                &area,
                &layout,
            );
            let placement = Placement {
                item_id: item.id.clone(),
                x: panel_type.trimming,
                y: panel_type.trimming,
                width: item.width,
                height: item.height,
                rotated: false,
            };
            best = Some((placement, score));
        }

        if item.can_rotate && item.height <= usable_width && item.width <= usable_height {
            let score = self.calculate_placement_score(
                area.x,
                area.y,
                item.height,
                item.width,
                &area,
                &layout,
            );
            let placement = Placement {
                item_id: item.id.clone(),
                x: panel_type.trimming,
                y: panel_type.trimming,
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

        best
    }

    /// Estimates how many copies of an item could fit on a fresh panel orientation.
    fn estimate_panel_capacity(
        &self,
        item: &Item,
        panel_width: f64,
        panel_height: f64,
        trimming: f64,
    ) -> u32 {
        let usable_width = panel_width - (trimming * 2.0);
        let usable_height = panel_height - (trimming * 2.0);

        if usable_width <= 0.0 || usable_height <= 0.0 {
            return 0;
        }

        let capacity_normal =
            self.capacity_for_dims(item.width, item.height, usable_width, usable_height);
        let capacity_rotated = if item.can_rotate {
            self.capacity_for_dims(item.height, item.width, usable_width, usable_height)
        } else {
            0
        };

        capacity_normal.max(capacity_rotated)
    }

    fn capacity_for_dims(
        &self,
        item_width: f64,
        item_height: f64,
        usable_width: f64,
        usable_height: f64,
    ) -> u32 {
        if item_width <= 0.0
            || item_height <= 0.0
            || item_width > usable_width
            || item_height > usable_height
        {
            return 0;
        }

        let step_w = item_width + self.request.cut_width;
        let step_h = item_height + self.request.cut_width;

        if step_w <= 0.0 || step_h <= 0.0 {
            return 0;
        }

        let cols = ((usable_width + self.request.cut_width) / step_w).floor() as u32;
        let rows = ((usable_height + self.request.cut_width) / step_h).floor() as u32;
        cols.saturating_mul(rows)
    }
}
