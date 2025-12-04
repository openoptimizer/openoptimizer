use super::*;
use crate::types::OptionalItem;
use std::cmp::Ordering;

/// Minimum effective waste percentage required before optional items are considered.
const MIN_WASTE_THRESHOLD_PERCENT: f64 = 8.0;

impl Optimizer {
    /// Attempts to insert optional items to reduce waste on already packed panels.
    /// Optional items are only considered when effective waste exceeds 8%.
    /// Items are tried in descending priority order.
    pub(super) fn try_add_optional_items(
        &self,
        layouts: Vec<PanelLayout>,
    ) -> Result<(Vec<PanelLayout>, Vec<String>)> {
        let initial_summary = self.calculate_summary(&layouts);
        let effective_waste_pct = initial_summary
            .actual_waste_percentage
            .unwrap_or(initial_summary.waste_percentage);

        // Only consider optional items if effective waste exceeds threshold
        if effective_waste_pct <= MIN_WASTE_THRESHOLD_PERCENT {
            return Ok((layouts, Vec::new()));
        }

        // Collect and sort optional items by priority (descending)
        let mut optional_items_pool: Vec<(String, OptionalItem)> = Vec::new();
        for panel_type in &self.request.panel_types {
            for item in &panel_type.optional_items {
                optional_items_pool.push((panel_type.id.clone(), item.clone()));
            }
        }

        if optional_items_pool.is_empty() {
            return Ok((layouts, Vec::new()));
        }

        // Sort by priority descending (higher priority first)
        optional_items_pool.sort_by(|a, b| b.1.priority.cmp(&a.1.priority));

        let mut best_layouts = layouts.clone();
        let mut best_summary = initial_summary;
        let mut items_used: Vec<String> = Vec::new();

        for (panel_type_id, optional_item) in optional_items_pool {
            let test_item = self.optional_to_item(&optional_item);
            let mut test_layouts = best_layouts.clone();
            let mut placed = false;

            for layout in &mut test_layouts {
                if layout.panel_type_id == panel_type_id {
                    if let Some(placement) = self.try_place_item(&test_item, layout) {
                        layout.placements.push(placement);
                        placed = true;
                        break;
                    }
                }
            }

            if placed {
                let test_summary = self.calculate_summary(&test_layouts);
                let best_waste = best_summary
                    .actual_waste_area
                    .unwrap_or(best_summary.waste_area);
                let test_waste = test_summary
                    .actual_waste_area
                    .unwrap_or(test_summary.waste_area);

                if test_waste < best_waste {
                    best_layouts = test_layouts;
                    best_summary = test_summary;
                    items_used.push(optional_item.id.clone());
                }
            }
        }

        Ok((best_layouts, items_used))
    }

    /// Returns the best scoring placement for an item on a layout, if any.
    pub(super) fn try_place_item(&self, item: &Item, layout: &PanelLayout) -> Option<Placement> {
        let candidates = self.generate_candidate_placements(item, layout);
        candidates
            .into_iter()
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal))
            .map(|(placement, _, _, _)| placement)
    }

    /// Converts an OptionalItem to an Item for placement logic.
    fn optional_to_item(&self, opt: &OptionalItem) -> Item {
        Item {
            id: opt.id.clone(),
            width: opt.width,
            height: opt.height,
            quantity: 1,
            can_rotate: opt.can_rotate,
        }
    }
}
