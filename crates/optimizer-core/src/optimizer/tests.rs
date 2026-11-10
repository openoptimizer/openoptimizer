use super::*;

fn panel(id: &str, width: f64, height: f64, trimming: f64) -> PanelType {
    PanelType {
        id: id.to_string(),
        width,
        height,
        trimming,
        optional_items: vec![],
    }
}

fn item(id: &str, width: f64, height: f64, quantity: u32, can_rotate: bool) -> Item {
    Item {
        id: id.to_string(),
        width,
        height,
        quantity,
        can_rotate,
    }
}

fn request(cut_width: f64, panel_types: Vec<PanelType>, items: Vec<Item>) -> OptimizationRequest {
    OptimizationRequest {
        cut_width,
        panel_types,
        items,
        min_initial_usage: true,
        min_reusable_remnant_size: None,
        optimize_for_reusable_remnants: true,
        time_limit_ms: Some(2000),
    }
}

fn run(request: OptimizationRequest) -> OptimizationResult {
    Optimizer::new(request).unwrap().optimize().unwrap()
}

/// Checks bounds, kerf distance between all pieces and piece counts.
fn assert_valid(request: &OptimizationRequest, result: &OptimizationResult) {
    let kerf = request.cut_width;
    let mut placed_required = 0;
    for layout in &result.layouts {
        let t = layout.trimming;
        for p in &layout.placements {
            assert!(
                p.x >= t - 1e-6 && p.y >= t - 1e-6,
                "{p:?} starts in trimming"
            );
            assert!(
                p.x + p.width <= layout.width - t + 1e-6,
                "{p:?} exceeds width"
            );
            assert!(
                p.y + p.height <= layout.height - t + 1e-6,
                "{p:?} exceeds height"
            );
        }
        for (i, a) in layout.placements.iter().enumerate() {
            for b in &layout.placements[i + 1..] {
                let gap_x = (b.x - (a.x + a.width)).max(a.x - (b.x + b.width));
                let gap_y = (b.y - (a.y + a.height)).max(a.y - (b.y + b.height));
                assert!(
                    gap_x.max(gap_y) >= kerf - 1e-6,
                    "{} and {} are closer than the kerf ({:.2})",
                    a.item_id,
                    b.item_id,
                    gap_x.max(gap_y)
                );
            }
        }
        placed_required += layout
            .placements
            .iter()
            .filter(|p| !result.optional_items_used.contains(&p.item_id))
            .count();
    }
    let expected: u32 = request.items.iter().map(|i| i.quantity).sum();
    assert_eq!(placed_required as u32, expected);
    let stats = result.statistics.as_ref().unwrap();
    assert!(stats.guillotine, "plan must be cuttable with through-cuts");
    assert!(result.summary.total_area + 1e-6 >= stats.lower_bound_area);
}

#[test]
fn keeps_full_kerf_on_every_side_of_every_piece() {
    // Regression: the kerf used to be added only to the right/top of a piece, so later pieces
    // placed left of or below an existing one could end up closer than one kerf.
    let request = request(
        4.0,
        vec![panel("hpl", 1854.0, 4100.0, 15.0)],
        vec![
            item("door", 616.0, 1857.0, 7, true),
            item("end", 291.0, 1853.0, 5, true),
            item("panel", 1180.0, 1853.0, 3, true),
            item("cap", 1180.0, 150.0, 4, true),
            item("small", 140.0, 1853.0, 6, true),
        ],
    );
    let result = run(request.clone());
    assert_valid(&request, &result);
}

#[test]
fn packs_three_doors_across_the_panel_width_with_a_thin_kerf() {
    // 3 x 606 + 2 x 0.3 = 1818.6 fits into 1854 - 2 x 15 = 1824.
    let request = request(
        0.3,
        vec![panel("hpl", 1854.0, 4100.0, 15.0)],
        vec![item("door", 606.0, 1907.0, 6, true)],
    );
    let result = run(request.clone());
    assert_valid(&request, &result);
    assert_eq!(result.summary.total_panels, 1);
}

#[test]
fn selects_the_smaller_panel_format_automatically() {
    let request = request(
        4.0,
        vec![
            panel("large", 1854.0, 4100.0, 15.0),
            panel("small", 1854.0, 2800.0, 15.0),
        ],
        vec![item("wall", 600.0, 2000.0, 3, true)],
    );
    let result = run(request.clone());
    assert_valid(&request, &result);
    assert_eq!(result.layouts.len(), 1);
    assert_eq!(result.layouts[0].panel_type_id, "small");
}

#[test]
fn mixes_formats_when_that_needs_less_material() {
    // Nine 1824 x 1300 blocks: two per large panel, one per small panel.
    let request = request(
        0.0,
        vec![
            panel("large", 1824.0, 2600.0, 0.0),
            panel("small", 1824.0, 1300.0, 0.0),
        ],
        vec![item("block", 1824.0, 1300.0, 9, false)],
    );
    let result = run(request.clone());
    assert_valid(&request, &result);
    assert!((result.summary.total_area - 9.0 * 1824.0 * 1300.0).abs() < 1.0);
    assert_eq!(result.summary.waste_area.round(), 0.0);
}

#[test]
fn respects_items_that_must_not_rotate_and_keeps_panel_orientation() {
    let request = request(
        3.0,
        vec![panel("panel", 1000.0, 500.0, 0.0)],
        vec![item("grain", 300.0, 450.0, 3, false)],
    );
    let result = run(request.clone());
    assert_valid(&request, &result);
    for layout in &result.layouts {
        assert_eq!((layout.width, layout.height), (1000.0, 500.0));
        for p in &layout.placements {
            assert!(!p.rotated);
            assert_eq!((p.width, p.height), (300.0, 450.0));
        }
    }
}

#[test]
fn trimming_offsets_placements_and_reduces_usable_area() {
    let request = request(
        0.0,
        vec![panel("panel", 120.0, 120.0, 10.0)],
        vec![item("square", 100.0, 100.0, 2, false)],
    );
    let result = run(request.clone());
    assert_valid(&request, &result);
    assert_eq!(result.summary.total_panels, 2);
    assert!(result.layouts.iter().all(|l| l.placements[0].x == 10.0));
}

#[test]
fn reports_guillotine_cuts_and_disjoint_offcuts() {
    let request = request(
        4.0,
        vec![panel("panel", 2000.0, 1000.0, 10.0)],
        vec![
            item("a", 600.0, 400.0, 3, true),
            item("b", 300.0, 300.0, 2, true),
        ],
    );
    let result = run(request.clone());
    assert_valid(&request, &result);
    let layout = &result.layouts[0];
    assert!(layout.cuts.iter().any(|c| c.stage == 1));
    let used: f64 = layout.placements.iter().map(|p| p.width * p.height).sum();
    let free: f64 = layout.unused_areas.iter().map(|a| a.width * a.height).sum();
    assert!(used + free <= 1980.0 * 980.0 + 1e-6);
    let overlaps = |ax: f64, aw: f64, ay: f64, ah: f64, bx: f64, bw: f64, by: f64, bh: f64| {
        let ox = (ax + aw).min(bx + bw) - ax.max(bx);
        let oy = (ay + ah).min(by + bh) - ay.max(by);
        ox > 1e-6 && oy > 1e-6
    };
    for (i, a) in layout.unused_areas.iter().enumerate() {
        for b in &layout.unused_areas[i + 1..] {
            assert!(!overlaps(
                a.x, a.width, a.y, a.height, b.x, b.width, b.y, b.height
            ));
        }
        for p in &layout.placements {
            assert!(!overlaps(
                a.x, a.width, a.y, a.height, p.x, p.width, p.y, p.height
            ));
        }
    }
}

#[test]
fn counts_large_offcuts_as_reusable_remnants() {
    let mut request = request(
        0.0,
        vec![panel("panel", 1000.0, 1000.0, 0.0)],
        vec![item("strip", 1000.0, 200.0, 1, false)],
    );
    request.min_reusable_remnant_size = Some(500_000.0);
    let result = run(request);
    assert_eq!(result.summary.reusable_remnant_area, Some(800_000.0));
    assert_eq!(result.summary.actual_waste_area, Some(0.0));
}

#[test]
fn places_optional_items_only_in_offcuts_and_without_new_panels() {
    let mut panel_type = panel("panel", 1000.0, 1000.0, 0.0);
    panel_type.optional_items = vec![
        OptionalItem {
            id: "spare".to_string(),
            width: 400.0,
            height: 400.0,
            can_rotate: true,
            priority: 5,
        },
        OptionalItem {
            id: "too-big".to_string(),
            width: 1200.0,
            height: 100.0,
            can_rotate: true,
            priority: 10,
        },
    ];
    let request = request(
        2.0,
        vec![panel_type],
        vec![item("main", 500.0, 1000.0, 1, false)],
    );
    let result = run(request.clone());
    assert_valid(&request, &result);
    assert_eq!(result.summary.total_panels, 1);
    assert_eq!(result.optional_items_used, vec!["spare".to_string(); 2]);
}

#[test]
fn expands_quantities_with_numbered_ids() {
    let request = request(
        3.0,
        vec![panel("panel", 1000.0, 1000.0, 0.0)],
        vec![
            item("a", 100.0, 100.0, 2, true),
            item("b", 50.0, 50.0, 1, true),
        ],
    );
    let result = run(request);
    let mut ids: Vec<String> = result.layouts[0]
        .placements
        .iter()
        .map(|p| p.item_id.clone())
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["a_1", "a_2", "b"]);
}

#[test]
fn produces_identical_results_for_identical_requests() {
    let request = request(
        4.0,
        vec![
            panel("large", 1854.0, 4100.0, 15.0),
            panel("small", 1300.0, 2800.0, 15.0),
        ],
        vec![
            item("a", 606.0, 1813.0, 9, true),
            item("b", 1480.0, 1810.0, 4, true),
            item("c", 133.0, 1813.0, 5, true),
        ],
    );
    let first = run(request.clone());
    let second = run(request);
    assert_eq!(
        serde_json::to_string(&first.layouts).unwrap(),
        serde_json::to_string(&second.layouts).unwrap()
    );
}

#[test]
fn rejects_items_larger_than_every_panel() {
    let request = request(
        3.0,
        vec![panel("panel", 500.0, 500.0, 0.0)],
        vec![item("huge", 600.0, 100.0, 1, false)],
    );
    let error = Optimizer::new(request).unwrap().optimize().unwrap_err();
    assert!(matches!(error, OptimizerError::ItemDoesNotFit(id) if id == "huge"));
}

#[test]
fn rejects_invalid_trimming_and_dimensions() {
    assert!(Optimizer::new(request(
        3.0,
        vec![panel("panel", 100.0, 100.0, 60.0)],
        vec![item("a", 10.0, 10.0, 1, true)],
    ))
    .is_err());
    assert!(Optimizer::new(request(
        3.0,
        vec![panel("panel", 100.0, 100.0, 0.0)],
        vec![item("a", 0.0, 10.0, 1, true)],
    ))
    .is_err());
}

#[test]
fn lower_bound_proves_plans_where_large_parts_need_their_own_panel() {
    // No two 1100 x 2053 parts fit on one 1854 x 2800 panel (nor on 1854 x 4100).
    let request = request(
        0.3,
        vec![
            panel("small", 1854.0, 2800.0, 15.0),
            panel("large", 1854.0, 4100.0, 15.0),
        ],
        vec![item("wall", 1100.0, 2053.0, 4, true)],
    );
    let result = run(request.clone());
    assert_valid(&request, &result);
    let bound = result.statistics.unwrap().lower_bound_area;
    assert!((bound - 4.0 * 1854.0 * 2800.0).abs() < 1.0);
    assert!((result.summary.total_area - bound).abs() < 1.0);
}
