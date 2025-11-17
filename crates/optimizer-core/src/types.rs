use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Panel type - describes an available panel size/type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelType {
    pub id: String,
    pub width: f64,
    pub height: f64,
    /// Uniform border trimmed from every edge before the panel is usable
    #[serde(default)]
    pub trimming: f64,
    /// Optional items that can be added to this panel type to reduce waste
    /// These are tested during optimization to see if they improve efficiency
    #[serde(default)]
    pub optional_items: Vec<Item>,
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
}

/// Error type for optimization
#[derive(Debug, thiserror::Error)]
pub enum OptimizerError {
    #[error("Cannot fit all items - need more/larger panels")]
    CannotFitAll,

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

pub type Result<T> = std::result::Result<T, OptimizerError>;
