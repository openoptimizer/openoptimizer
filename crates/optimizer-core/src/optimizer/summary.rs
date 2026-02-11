use super::*;
use crate::types::UnusedArea as OutputUnusedArea;
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
                    // compute_output_unused_areas can still contain overlaps. Select a
                    // non-overlapping set (largest first) to avoid double counting.
                    let mut unused_areas = self.compute_output_unused_areas(layout);
                    unused_areas.sort_by(|a, b| {
                        let area_a = a.width * a.height;
                        let area_b = b.width * b.height;
                        area_b
                            .partial_cmp(&area_a)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let mut accepted: Vec<OutputUnusedArea> = Vec::new();

                    for area in unused_areas {
                        let area_size = area.width * area.height;
                        if area_size < min_size {
                            continue;
                        }

                        if accepted
                            .iter()
                            .any(|existing| areas_overlap(existing, &area))
                        {
                            continue;
                        }

                        reusable_area += area_size;
                        accepted.push(area);
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

fn areas_overlap(a: &OutputUnusedArea, b: &OutputUnusedArea) -> bool {
    let eps = 0.5;
    a.x < b.x + b.width - eps
        && a.x + a.width > b.x + eps
        && a.y < b.y + b.height - eps
        && a.y + a.height > b.y + eps
}
