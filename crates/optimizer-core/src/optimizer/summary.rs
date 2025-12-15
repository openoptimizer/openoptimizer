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
    pub(super) fn calculate_summary(&self, layouts: &[PanelLayout]) -> Summary {
        let total_panels = layouts.len() as u32;
        let total_area: f64 = layouts.iter().map(|l| l.width * l.height).sum();
        let used_area: f64 = layouts
            .iter()
            .flat_map(|l| &l.placements)
            .map(|p| p.width * p.height)
            .sum();
        let waste_area = total_area - used_area;
        let waste_percentage = if total_area > 0.0 {
            (waste_area / total_area) * 100.0
        } else {
            0.0
        };

        let (reusable_remnant_area, actual_waste_area, actual_waste_percentage) =
            if let Some(min_size) = self.request.min_reusable_remnant_size {
                let mut reusable_area = 0.0;

                for layout in layouts {
                    // Use compute_output_unused_areas which returns non-overlapping rectangles
                    // instead of find_unused_areas which returns overlapping maximal rectangles
                    let unused_areas = self.compute_output_unused_areas(layout);
                    for area in unused_areas {
                        let area_size = area.width * area.height;
                        if area_size >= min_size {
                            reusable_area += area_size;
                        }
                    }
                }

                // Clamp reusable_area to not exceed waste_area to avoid negative actual_waste
                let reusable_area = reusable_area.min(waste_area);

                let actual_waste = waste_area - reusable_area;
                let actual_waste_pct = if total_area > 0.0 {
                    (actual_waste / total_area) * 100.0
                } else {
                    0.0
                };

                (
                    Some(reusable_area),
                    Some(actual_waste),
                    Some(actual_waste_pct),
                )
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
