use super::*;
use crate::types::UnusedArea as OutputUnusedArea;

/// Internal representation of unused areas during computation.
/// Uses maxrects-style free rectangle tracking for efficient packing.
#[derive(Debug, Clone)]
pub(super) struct UnusedArea {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl UnusedArea {
    /// Returns true if this rectangle fully contains another rectangle.
    fn contains(&self, other: &UnusedArea, eps: f64) -> bool {
        other.x >= self.x - eps
            && other.y >= self.y - eps
            && other.x + other.width <= self.x + self.width + eps
            && other.y + other.height <= self.y + self.height + eps
    }

    /// Returns true if two rectangles overlap.
    fn overlaps(&self, other: &UnusedArea) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

impl Optimizer {
    /// Returns every rectangular area that is still free on the panel.
    /// Uses maxrects algorithm: maintains a set of maximal free rectangles.
    /// This produces better packing by tracking all possible placement positions.
    pub(super) fn find_unused_areas(&self, layout: &PanelLayout) -> Vec<UnusedArea> {
        let usable_width = layout.width - (layout.trimming * 2.0);
        let usable_height = layout.height - (layout.trimming * 2.0);

        if usable_width <= 0.0 || usable_height <= 0.0 {
            return Vec::new();
        }

        // Start with the full usable area
        let mut free_rects = vec![UnusedArea {
            x: layout.trimming,
            y: layout.trimming,
            width: usable_width,
            height: usable_height,
        }];

        // For each placed item, split any overlapping free rectangles
        for placement in &layout.placements {
            let placed_rect = UnusedArea {
                x: placement.x,
                y: placement.y,
                width: placement.width + self.request.cut_width,
                height: placement.height + self.request.cut_width,
            };

            free_rects = self.split_free_rects_around_placement(free_rects, &placed_rect);
        }

        // Remove rectangles that are too small to be useful
        free_rects
            .into_iter()
            .filter(|r| r.width > 0.5 && r.height > 0.5)
            .collect()
    }

    /// Splits free rectangles around a placed item using maxrects algorithm.
    /// This creates up to 4 new rectangles for each overlapping free rect,
    /// then removes any rectangles that are fully contained by others.
    fn split_free_rects_around_placement(
        &self,
        free_rects: Vec<UnusedArea>,
        placed: &UnusedArea,
    ) -> Vec<UnusedArea> {
        let mut new_free_rects = Vec::new();

        for rect in free_rects {
            if !rect.overlaps(placed) {
                new_free_rects.push(rect);
                continue;
            }

            // Generate up to 4 maximal rectangles from the remaining space
            // Left piece: from rect left edge to placed left edge
            if placed.x > rect.x {
                new_free_rects.push(UnusedArea {
                    x: rect.x,
                    y: rect.y,
                    width: placed.x - rect.x,
                    height: rect.height,
                });
            }

            // Right piece: from placed right edge to rect right edge
            let placed_right = placed.x + placed.width;
            let rect_right = rect.x + rect.width;
            if placed_right < rect_right {
                new_free_rects.push(UnusedArea {
                    x: placed_right,
                    y: rect.y,
                    width: rect_right - placed_right,
                    height: rect.height,
                });
            }

            // Bottom piece: from rect bottom edge to placed bottom edge
            if placed.y > rect.y {
                new_free_rects.push(UnusedArea {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: placed.y - rect.y,
                });
            }

            // Top piece: from placed top edge to rect top edge
            let placed_top = placed.y + placed.height;
            let rect_top = rect.y + rect.height;
            if placed_top < rect_top {
                new_free_rects.push(UnusedArea {
                    x: rect.x,
                    y: placed_top,
                    width: rect.width,
                    height: rect_top - placed_top,
                });
            }
        }

        // Remove rectangles that are fully contained by other rectangles
        self.prune_contained_rects(new_free_rects)
    }

    /// Removes rectangles that are fully contained within other rectangles.
    /// This keeps only the maximal free rectangles.
    fn prune_contained_rects(&self, rects: Vec<UnusedArea>) -> Vec<UnusedArea> {
        let eps = 0.5;
        let mut result = Vec::new();

        for (i, rect) in rects.iter().enumerate() {
            let is_contained = rects
                .iter()
                .enumerate()
                .any(|(j, other)| i != j && other.contains(rect, eps));

            if !is_contained {
                result.push(rect.clone());
            }
        }

        result
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
