use super::*;
use std::collections::HashMap;

impl Optimizer {
    /// Aggregates how many panels of each type were consumed.
    pub(super) fn count_panels(&self, layouts: &[PanelLayout]) -> HashMap<String, u32> {
        let mut counts = HashMap::new();
        for layout in layouts {
            *counts.entry(layout.panel_type_id.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// Computes utilization, waste, and optional remnant statistics.
    /// Unused areas are disjoint offcuts of the cut tree, so they can be summed directly.
    pub(super) fn calculate_summary(&self, layouts: &[PanelLayout]) -> Summary {
        let total_panels = layouts.len() as u32;
        let total_area: f64 = layouts.iter().map(|l| l.width * l.height).sum();
        let used_area: f64 = layouts
            .iter()
            .flat_map(|l| &l.placements)
            .map(|p| p.width * p.height)
            .sum();
        let waste_area = total_area - used_area;
        let percent = |value: f64| {
            if total_area > 0.0 {
                value / total_area * 100.0
            } else {
                0.0
            }
        };
        let waste_percentage = percent(waste_area);

        let (reusable_remnant_area, actual_waste_area, actual_waste_percentage) =
            if let Some(min_size) = self.request.min_reusable_remnant_size {
                let reusable: f64 = layouts
                    .iter()
                    .flat_map(|l| &l.unused_areas)
                    .map(|a| a.width * a.height)
                    .filter(|area| *area >= min_size)
                    .sum();
                let reusable = reusable.min(waste_area);
                let actual = waste_area - reusable;
                (Some(reusable), Some(actual), Some(percent(actual)))
            } else {
                (None, None, None)
            };

        Summary {
            total_panels,
            total_area,
            used_area,
            waste_area,
            waste_percentage,
            reusable_remnant_area,
            actual_waste_area,
            actual_waste_percentage,
        }
    }
}
