use super::*;

#[derive(Debug, Clone)]
pub(super) struct UnusedArea {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Optimizer {
    /// Returns every rectangular area that is still free on the panel.
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
}
