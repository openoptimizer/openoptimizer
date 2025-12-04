use super::*;
use crate::types::UnusedArea as OutputUnusedArea;

/// Internal representation of unused areas during computation.
#[derive(Debug, Clone)]
pub(super) struct UnusedArea {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Optimizer {
    /// Returns every rectangular area that is still free on the panel.
    /// Used internally for candidate placement generation.
    pub(super) fn find_unused_areas(&self, layout: &PanelLayout) -> Vec<UnusedArea> {
        let usable_width = layout.width - (layout.trimming * 2.0);
        let usable_height = layout.height - (layout.trimming * 2.0);

        if usable_width <= 0.0 || usable_height <= 0.0 {
            return Vec::new();
        }

        let mut candidates = vec![UnusedArea {
            x: layout.trimming,
            y: layout.trimming,
            width: usable_width,
            height: usable_height,
        }];

        for placement in &layout.placements {
            let mut new_candidates = Vec::new();

            for area in candidates {
                let overlap_x1 = area.x.max(placement.x);
                let overlap_x2 = (placement.x + placement.width + self.request.cut_width)
                    .min(area.x + area.width);
                let overlap_y1 = area.y.max(placement.y);
                let overlap_y2 = (placement.y + placement.height + self.request.cut_width)
                    .min(area.y + area.height);

                let overlaps = overlap_x1 < overlap_x2 && overlap_y1 < overlap_y2;

                if !overlaps {
                    new_candidates.push(area);
                    continue;
                }

                if overlap_y1 > area.y {
                    new_candidates.push(UnusedArea {
                        x: area.x,
                        y: area.y,
                        width: area.width,
                        height: overlap_y1 - area.y,
                    });
                }

                if overlap_y2 < area.y + area.height {
                    new_candidates.push(UnusedArea {
                        x: area.x,
                        y: overlap_y2,
                        width: area.width,
                        height: area.y + area.height - overlap_y2,
                    });
                }

                if overlap_x1 > area.x && overlap_y2 > overlap_y1 {
                    new_candidates.push(UnusedArea {
                        x: area.x,
                        y: overlap_y1,
                        width: overlap_x1 - area.x,
                        height: overlap_y2 - overlap_y1,
                    });
                }

                if overlap_x2 < area.x + area.width && overlap_y2 > overlap_y1 {
                    new_candidates.push(UnusedArea {
                        x: overlap_x2,
                        y: overlap_y1,
                        width: area.x + area.width - overlap_x2,
                        height: overlap_y2 - overlap_y1,
                    });
                }
            }

            candidates = new_candidates;
        }

        candidates
            .into_iter()
            .filter(|area| area.width > 0.0 && area.height > 0.0)
            .collect()
    }

    /// Returns simplified rectangular unused areas for output.
    /// Merges overlapping regions and prefers rectangles with the largest area (sq meters).
    pub(super) fn compute_output_unused_areas(
        &self,
        layout: &PanelLayout,
    ) -> Vec<OutputUnusedArea> {
        let raw_areas = self.find_unused_areas(layout);

        // Merge overlapping/adjacent areas by preferring the largest rectangles
        let merged = self.merge_unused_areas(raw_areas);

        merged
            .into_iter()
            .map(|a| OutputUnusedArea {
                x: a.x,
                y: a.y,
                width: a.width,
                height: a.height,
            })
            .collect()
    }

    /// Merges adjacent unused areas into larger rectangles when possible.
    /// Combines perfectly aligned adjacent rectangles to form larger pieces.
    fn merge_unused_areas(&self, areas: Vec<UnusedArea>) -> Vec<UnusedArea> {
        if areas.is_empty() {
            return Vec::new();
        }

        let eps = 1.0; // tolerance in mm for alignment

        let mut working = areas;

        // Phase 1: Merge perfectly aligned adjacent rectangles (multiple passes)
        loop {
            let mut merged_any = false;
            let mut new_working: Vec<UnusedArea> = Vec::new();
            let mut used: Vec<bool> = vec![false; working.len()];

            for i in 0..working.len() {
                if used[i] {
                    continue;
                }

                let mut current = working[i].clone();
                used[i] = true;

                for j in (i + 1)..working.len() {
                    if used[j] {
                        continue;
                    }

                    let other = &working[j];

                    // Vertically adjacent with same x and width
                    let same_x = (current.x - other.x).abs() < eps;
                    let same_width = (current.width - other.width).abs() < eps;
                    let v_adjacent_above = (current.y - (other.y + other.height)).abs() < eps;
                    let v_adjacent_below = (other.y - (current.y + current.height)).abs() < eps;

                    if same_x && same_width && (v_adjacent_above || v_adjacent_below) {
                        let new_y = current.y.min(other.y);
                        let new_height = current.height + other.height;
                        current = UnusedArea {
                            x: current.x,
                            y: new_y,
                            width: current.width,
                            height: new_height,
                        };
                        used[j] = true;
                        merged_any = true;
                        continue;
                    }

                    // Horizontally adjacent with same y and height
                    let same_y = (current.y - other.y).abs() < eps;
                    let same_height = (current.height - other.height).abs() < eps;
                    let h_adjacent_left = (current.x - (other.x + other.width)).abs() < eps;
                    let h_adjacent_right = (other.x - (current.x + current.width)).abs() < eps;

                    if same_y && same_height && (h_adjacent_left || h_adjacent_right) {
                        let new_x = current.x.min(other.x);
                        let new_width = current.width + other.width;
                        current = UnusedArea {
                            x: new_x,
                            y: current.y,
                            width: new_width,
                            height: current.height,
                        };
                        used[j] = true;
                        merged_any = true;
                        continue;
                    }
                }

                new_working.push(current);
            }

            working = new_working;

            if !merged_any {
                break;
            }
        }

        // Phase 2: Filter out smaller rectangles that are fully contained within larger ones
        let mut result: Vec<UnusedArea> = Vec::new();

        // Sort by area descending
        working.sort_by(|a, b| {
            let area_a = a.width * a.height;
            let area_b = b.width * b.height;
            area_b
                .partial_cmp(&area_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for area in working {
            // Skip tiny areas (less than 10mm in either dimension)
            if area.width < 10.0 || area.height < 10.0 {
                continue;
            }

            // Check if this area is fully contained in any existing result
            let is_contained = result.iter().any(|existing| {
                area.x >= existing.x - eps
                    && area.y >= existing.y - eps
                    && area.x + area.width <= existing.x + existing.width + eps
                    && area.y + area.height <= existing.y + existing.height + eps
            });

            if is_contained {
                continue;
            }

            result.push(area);
        }

        result
    }
}
