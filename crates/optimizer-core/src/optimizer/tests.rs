use super::*;

#[test]
fn test_simple_optimization() {
    let request = OptimizationRequest {
        cut_width: 3.0,
        panel_types: vec![PanelType {
            id: "panel_a".to_string(),
            width: 100.0,
            height: 100.0,
            optional_items: vec![],
        }],
        items: vec![
            Item {
                id: "item1".to_string(),
                width: 20.0,
                height: 30.0,
                quantity: 2,
                can_rotate: true,
            },
            Item {
                id: "item2".to_string(),
                width: 40.0,
                height: 50.0,
                quantity: 1,
                can_rotate: false,
            },
        ],
        min_initial_usage: false,
        min_reusable_remnant_size: None,
        optimize_for_reusable_remnants: false,
    };

    let optimizer = Optimizer::new(request).unwrap();
    let result = optimizer.optimize().unwrap();

    assert!(result.summary.total_panels >= 1);
    assert!(result.summary.waste_percentage >= 0.0);
    assert!(result.summary.waste_percentage <= 100.0);
    assert_eq!(result.layouts.len(), result.summary.total_panels as usize);
}

#[test]
fn test_reusable_remnant_size() {
    let request = OptimizationRequest {
        cut_width: 3.0,
        panel_types: vec![PanelType {
            id: "panel_a".to_string(),
            width: 1000.0,
            height: 1000.0,
            optional_items: vec![],
        }],
        items: vec![Item {
            id: "item1".to_string(),
            width: 200.0,
            height: 200.0,
            quantity: 1,
            can_rotate: false,
        }],
        min_initial_usage: false,
        min_reusable_remnant_size: Some(10000.0),
        optimize_for_reusable_remnants: false,
    };

    let optimizer = Optimizer::new(request).unwrap();
    let result = optimizer.optimize().unwrap();

    assert!(result.summary.reusable_remnant_area.is_some());
    assert!(result.summary.actual_waste_area.is_some());
    assert!(result.summary.actual_waste_percentage.is_some());

    let reusable = result.summary.reusable_remnant_area.unwrap();
    assert!(reusable > 0.0);

    let total_waste = result.summary.waste_area;
    let actual_waste = result.summary.actual_waste_area.unwrap();
    assert!(actual_waste < total_waste);
}

#[test]
fn test_optimize_for_reusable_remnants() {
    let request = OptimizationRequest {
        cut_width: 3.0,
        panel_types: vec![PanelType {
            id: "panel_a".to_string(),
            width: 1000.0,
            height: 1000.0,
            optional_items: vec![],
        }],
        items: vec![Item {
            id: "item1".to_string(),
            width: 300.0,
            height: 300.0,
            quantity: 2,
            can_rotate: false,
        }],
        min_initial_usage: false,
        min_reusable_remnant_size: None,
        optimize_for_reusable_remnants: true,
    };

    let optimizer = Optimizer::new(request).unwrap();
    let result = optimizer.optimize().unwrap();

    assert!(result.summary.total_panels >= 1);
}

#[test]
fn test_min_initial_usage_packs_single_panel() {
    let request = OptimizationRequest {
        cut_width: 2.0,
        panel_types: vec![PanelType {
            id: "panel_a".to_string(),
            width: 2400.0,
            height: 1200.0,
            optional_items: vec![],
        }],
        items: vec![Item {
            id: "shelf".to_string(),
            width: 600.0,
            height: 300.0,
            quantity: 8,
            can_rotate: false,
        }],
        min_initial_usage: true,
        min_reusable_remnant_size: None,
        optimize_for_reusable_remnants: false,
    };

    let optimizer = Optimizer::new(request).unwrap();
    let result = optimizer.optimize().unwrap();

    assert_eq!(result.summary.total_panels, 1);
    assert_eq!(result.layouts.len(), 1);
    assert_eq!(result.layouts[0].placements.len(), 8);
}

#[test]
fn test_unused_area_allows_additional_row() {
    let request = OptimizationRequest {
        cut_width: 2.0,
        panel_types: vec![PanelType {
            id: "plywood".into(),
            width: 2400.0,
            height: 1200.0,
            optional_items: vec![],
        }],
        items: vec![Item {
            id: "dummy".into(),
            width: 100.0,
            height: 100.0,
            quantity: 1,
            can_rotate: false,
        }],
        min_initial_usage: true,
        min_reusable_remnant_size: None,
        optimize_for_reusable_remnants: false,
    };

    let optimizer = Optimizer::new(request).unwrap();

    let layout = PanelLayout {
        panel_type_id: "plywood".into(),
        panel_number: 1,
        width: 2400.0,
        height: 1200.0,
        placements: vec![
            Placement {
                item_id: "shelf1".into(),
                x: 0.0,
                y: 0.0,
                width: 600.0,
                height: 300.0,
                rotated: false,
            },
            Placement {
                item_id: "shelf2".into(),
                x: 602.0,
                y: 0.0,
                width: 600.0,
                height: 300.0,
                rotated: false,
            },
            Placement {
                item_id: "shelf3".into(),
                x: 1204.0,
                y: 0.0,
                width: 600.0,
                height: 300.0,
                rotated: false,
            },
            Placement {
                item_id: "shelf4".into(),
                x: 0.0,
                y: 302.0,
                width: 600.0,
                height: 300.0,
                rotated: false,
            },
            Placement {
                item_id: "shelf5".into(),
                x: 602.0,
                y: 302.0,
                width: 600.0,
                height: 300.0,
                rotated: false,
            },
            Placement {
                item_id: "shelf6".into(),
                x: 1204.0,
                y: 302.0,
                width: 600.0,
                height: 300.0,
                rotated: false,
            },
        ],
    };

    let areas = optimizer.find_unused_areas(&layout);
    assert!(areas.iter().any(|a| a.width >= 600.0 && a.height >= 300.0));
}

#[test]
fn test_try_place_item_after_vertical_piece() {
    let request = OptimizationRequest {
        cut_width: 2.0,
        panel_types: vec![PanelType {
            id: "plywood".into(),
            width: 2400.0,
            height: 1200.0,
            optional_items: vec![],
        }],
        items: vec![Item {
            id: "shelf".into(),
            width: 600.0,
            height: 300.0,
            quantity: 1,
            can_rotate: false,
        }],
        min_initial_usage: true,
        min_reusable_remnant_size: None,
        optimize_for_reusable_remnants: false,
    };

    let optimizer = Optimizer::new(request).unwrap();

    let layout = PanelLayout {
        panel_type_id: "plywood".into(),
        panel_number: 1,
        width: 2400.0,
        height: 1200.0,
        placements: vec![
            Placement {
                item_id: "shelf1".into(),
                x: 0.0,
                y: 0.0,
                width: 600.0,
                height: 300.0,
                rotated: false,
            },
            Placement {
                item_id: "shelf2".into(),
                x: 602.0,
                y: 0.0,
                width: 600.0,
                height: 300.0,
                rotated: false,
            },
            Placement {
                item_id: "shelf3".into(),
                x: 1204.0,
                y: 0.0,
                width: 600.0,
                height: 300.0,
                rotated: false,
            },
            Placement {
                item_id: "shelf4".into(),
                x: 1806.0,
                y: 0.0,
                width: 300.0,
                height: 600.0,
                rotated: true,
            },
            Placement {
                item_id: "shelf5".into(),
                x: 0.0,
                y: 602.0,
                width: 600.0,
                height: 300.0,
                rotated: false,
            },
            Placement {
                item_id: "shelf6".into(),
                x: 602.0,
                y: 602.0,
                width: 600.0,
                height: 300.0,
                rotated: false,
            },
            Placement {
                item_id: "shelf7".into(),
                x: 1204.0,
                y: 602.0,
                width: 600.0,
                height: 300.0,
                rotated: false,
            },
        ],
    };

    let next_item = Item {
        id: "shelf8".into(),
        width: 600.0,
        height: 300.0,
        quantity: 1,
        can_rotate: false,
    };

    let placement = optimizer.try_place_item(&next_item, &layout);
    assert!(placement.is_some());
}
