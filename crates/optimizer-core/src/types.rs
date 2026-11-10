use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Optional item that can be placed to reduce waste.
/// These are only considered when effective waste exceeds 8%.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalItem {
    pub id: String,
    pub width: f64,
    pub height: f64,
    pub can_rotate: bool,
    /// Higher priority items are tried first (default: 0)
    #[serde(default)]
    pub priority: i32,
}

/// Panel type - describes an available panel size/type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelType {
    pub id: String,
    pub width: f64,
    pub height: f64,
    /// Uniform border trimmed from every edge before the panel is usable
    #[serde(default)]
    pub trimming: f64,
    /// Optional filler items that can be placed to reduce waste.
    /// Only considered when effective waste exceeds 8%; sorted by priority (descending).
    #[serde(default)]
    pub optional_items: Vec<OptionalItem>,
}

/// Item to be cut
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub width: f64,
    pub height: f64,
    pub quantity: u32,
    pub can_rotate: bool,
}

/// Input: What user provides
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationRequest {
    pub cut_width: f64,
    pub panel_types: Vec<PanelType>,
    pub items: Vec<Item>,
    /// Minimize usage of initial panels (prioritize filling panels completely)
    #[serde(default)]
    pub min_initial_usage: bool,
    /// Minimum size (area) for a remnant to be considered reusable (not waste)
    /// Remnants larger than this can be reused and won't count as waste
    #[serde(default)]
    pub min_reusable_remnant_size: Option<f64>,
    /// Try to optimize for leaving large reusable remnants
    #[serde(default)]
    pub optimize_for_reusable_remnants: bool,
    /// Upper bound for the search time in milliseconds (default 3000, max 30000).
    /// The first solution is always computed completely; later improvement rounds stop here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_limit_ms: Option<u64>,
}

/// One saw cut of the guillotine cutting plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cut {
    /// 1 = first cuts through the whole (trimmed) panel, 2 = cuts through first-stage strips, ...
    pub stage: u32,
    /// `vertical` cuts run parallel to the y axis, `horizontal` cuts parallel to the x axis.
    pub direction: CutDirection,
    /// Start of the kerf (left edge for vertical, bottom edge for horizontal cuts).
    pub x: f64,
    pub y: f64,
    /// Length of the cut.
    pub length: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CutDirection {
    Vertical,
    Horizontal,
}

/// Information about the optimization run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statistics {
    pub algorithm: String,
    pub iterations: u32,
    pub elapsed_ms: u64,
    /// Every layout can be produced with straight through-cuts (panel saw).
    pub guillotine: bool,
    /// Highest cut stage used in any layout.
    pub max_stage: u32,
    /// Lower bound for the panel area (mm²): no plan can use less material.
    pub lower_bound_area: f64,
}

/// Placement of an item on a panel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Placement {
    pub item_id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotated: bool,
}

/// Rectangular unused (leftover) area on a panel after placements.
/// When free space has more than 4 edges, the rectangle with the largest area (sq meters) is preferred.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnusedArea {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Layout of a single panel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelLayout {
    pub panel_type_id: String,
    pub panel_number: u32,
    pub width: f64,
    pub height: f64,
    /// Trimming margin applied to this panel (same on all sides)
    #[serde(default)]
    pub trimming: f64,
    pub placements: Vec<Placement>,
    /// Rectangular unused (leftover) areas on this panel.
    /// Rectangles with the largest area (sq meters) are preferred when free space must be split.
    #[serde(default)]
    pub unused_areas: Vec<UnusedArea>,
    /// Guillotine saw cuts in cutting order (trimming cuts are not listed).
    #[serde(default)]
    pub cuts: Vec<Cut>,
}

/// Summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub total_panels: u32,
    pub total_area: f64,
    pub used_area: f64,
    pub waste_area: f64,
    pub waste_percentage: f64,
    /// Area of remnants that are large enough to be reusable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reusable_remnant_area: Option<f64>,
    /// Actual waste area (excluding reusable remnants)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_waste_area: Option<f64>,
    /// Actual waste percentage (excluding reusable remnants)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_waste_percentage: Option<f64>,
}

/// Output: What optimizer returns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    /// How many panels of each type are needed
    pub panels_required: HashMap<String, u32>,
    /// Detailed cutting layouts for each panel
    pub layouts: Vec<PanelLayout>,
    /// Overall statistics
    pub summary: Summary,
    /// Optional items that were used (if any)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub optional_items_used: Vec<String>,
    /// How the result was computed.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub statistics: Option<Statistics>,
}

/// Error type for optimization
#[derive(Debug, thiserror::Error)]
pub enum OptimizerError {
    #[error("Cannot fit all items - need more/larger panels")]
    CannotFitAll,

    #[error("Cannot fit all items - item '{0}' does not fit on any panel type")]
    ItemDoesNotFit(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

pub type Result<T> = std::result::Result<T, OptimizerError>;
