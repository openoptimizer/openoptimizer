use super::*;
use std::cmp::Ordering;

impl Optimizer {
    /// Attempts to insert optional items to reduce waste on already packed panels.
    pub(super) fn try_add_optional_items(
        &self,
        layouts: Vec<PanelLayout>,
    ) -> Result<(Vec<PanelLayout>, Vec<String>)> {
        let mut optional_items_pool: Vec<(String, Item)> = Vec::new();
        for panel_type in &self.request.panel_types {
            for item in &panel_type.optional_items {
                optional_items_pool.push((panel_type.id.clone(), item.clone()));
            }
        }

        if optional_items_pool.is_empty() {
            return Ok((layouts, Vec::new()));
        }

        let mut best_layouts = layouts.clone();
        let mut best_summary = self.calculate_summary(&best_layouts);
        let mut items_used: Vec<String> = Vec::new();

        for (panel_type_id, optional_item) in optional_items_pool {
            let expanded_optional = self.expand_single_item(&optional_item);
            let mut test_layouts = best_layouts.clone();
            let mut placed_count = 0;

            for expanded_item in &expanded_optional {
                for layout in &mut test_layouts {
                    if layout.panel_type_id == panel_type_id {
                        if let Some(placement) = self.try_place_item(expanded_item, layout) {
                            layout.placements.push(placement);
                            placed_count += 1;
                            break;
                        }
                    }
                }
            }

            if placed_count > 0 {
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
                    items_used.push(format!("{}x{}", placed_count, optional_item.id));
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

    /// Expands a single optional item into concrete pieces so they can be packed.
    fn expand_single_item(&self, item: &Item) -> Vec<Item> {
        let mut expanded = Vec::new();
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
        expanded
    }
}
