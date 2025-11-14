use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use optimizer_core::{OptimizationRequest, OptimizationResult, Optimizer, OptimizerError};
use serde_json::json;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

const OPENAPI_SPEC: &str = include_str!("../../../openapi.yaml");
const SWAGGER_UI_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>OpenOptimizer API Docs</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
    <script>
        window.onload = () => {
            SwaggerUIBundle({
                url: '/openapi.yaml',
                dom_id: '#swagger-ui',
                presets: [SwaggerUIBundle.presets.apis],
                layout: 'BaseLayout',
            });
        };
    </script>
</body>
</html>"#;

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting Cutting Optimizer API");

    // Build application
    let app = Router::new()
        .route("/", get(serve_ui))
        .route("/api/health", get(health_check))
        .route("/api/optimize", post(optimize))
        .route("/api/generate/svg", post(generate_svg))
        .route("/openapi.yaml", get(serve_openapi_spec))
        .route("/docs", get(serve_swagger_ui))
        .layer(CorsLayer::permissive());

    // Start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Failed to bind to port 3000");

    info!("API server listening on http://0.0.0.0:3000");
    info!("Try: curl http://localhost:3000/api/health");

    axum::serve(listener, app).await.expect("Server error");
}

/// Health check endpoint
async fn health_check() -> Json<serde_json::Value> {
    Json(json!({
        "status": "healthy",
        "service": "cutting-optimizer-api",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Main optimization endpoint
async fn optimize(
    Json(request): Json<OptimizationRequest>,
) -> Result<Json<OptimizationResult>, AppError> {
    info!(
        "Received optimization request with {} items and {} panel types",
        request.items.len(),
        request.panel_types.len()
    );

    let optimizer = Optimizer::new(request)?;
    let result = optimizer.optimize()?;

    info!(
        "Optimization complete: {} panels required, {:.2}% waste",
        result.summary.total_panels, result.summary.waste_percentage
    );

    Ok(Json(result))
}

/// Generate SVG visualization
async fn generate_svg(Json(result): Json<OptimizationResult>) -> Result<Response, AppError> {
    info!("Generating SVG for {} panels", result.layouts.len());

    let svg = generate_svg_content(&result)?;

    Ok((StatusCode::OK, [("Content-Type", "image/svg+xml")], svg).into_response())
}

/// Generate SVG content from optimization result
fn generate_svg_content(result: &OptimizationResult) -> Result<String, AppError> {
    use std::fmt::Write;

    let mut svg = String::new();
    let margin = 20.0;
    let scale = 2.0; // Scale down panels to fit in SVG
    let panel_spacing = 40.0;

    // Calculate total SVG size
    let max_width = result.layouts.iter().map(|l| l.width).fold(0.0, f64::max);
    let total_height: f64 = result
        .layouts
        .iter()
        .map(|l| l.height + panel_spacing)
        .sum();

    let svg_width = (max_width / scale) + (2.0 * margin);
    let svg_height = (total_height / scale) + (2.0 * margin);

    // SVG header
    writeln!(&mut svg, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    writeln!(
        &mut svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
        svg_width, svg_height, svg_width, svg_height
    )
    .unwrap();

    // Background
    writeln!(
        &mut svg,
        r##"  <rect width="100%" height="100%" fill="#f5f5f5"/>"##
    )
    .unwrap();

    let mut y_offset = margin;

    for layout in &result.layouts {
        let x = margin;
        let panel_width = layout.width / scale;
        let panel_height = layout.height / scale;

        // Draw panel background
        writeln!(&mut svg, r##"  <rect x="{}" y="{}" width="{}" height="{}" fill="#ffffff" stroke="#333" stroke-width="2"/>"##,
                 x, y_offset, panel_width, panel_height).unwrap();

        // Draw panel label
        writeln!(&mut svg, r##"  <text x="{}" y="{}" font-family="Arial" font-size="14" fill="#333">{} #{}</text>"##,
                 x, y_offset - 5.0, layout.panel_type_id, layout.panel_number).unwrap();

        // Draw placements
        for placement in &layout.placements {
            let px = x + (placement.x / scale);
            let py = y_offset + (placement.y / scale);
            let pw = placement.width / scale;
            let ph = placement.height / scale;

            // Draw item rectangle
            writeln!(&mut svg, r##"  <rect x="{}" y="{}" width="{}" height="{}" fill="#4CAF50" stroke="#2E7D32" stroke-width="1" opacity="0.7"/>"##,
                     px, py, pw, ph).unwrap();

            // Draw item label
            let label = if placement.rotated {
                format!("{} (R)", placement.item_id)
            } else {
                placement.item_id.clone()
            };

            writeln!(&mut svg, r##"  <text x="{}" y="{}" font-family="Arial" font-size="10" fill="#fff" text-anchor="middle">{}</text>"##,
                     px + pw / 2.0, py + ph / 2.0 + 3.0, label).unwrap();
        }

        y_offset += panel_height + panel_spacing;
    }

    // Summary
    writeln!(
        &mut svg,
        r##"  <text x="{}" y="{}" font-family="Arial" font-size="12" fill="#666">"##,
        margin,
        svg_height - margin + 15.0
    )
    .unwrap();
    writeln!(
        &mut svg,
        r#"    Panels: {} | Waste: {:.1}%"#,
        result.summary.total_panels, result.summary.waste_percentage
    )
    .unwrap();
    writeln!(&mut svg, r#"  </text>"#).unwrap();

    writeln!(&mut svg, "</svg>").unwrap();

    Ok(svg)
}

/// Application error type
struct AppError(anyhow::Error);

impl From<OptimizerError> for AppError {
    fn from(err: OptimizerError) -> Self {
        AppError(err.into())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        error!("Request error: {}", self.0);

        let message = self.0.to_string();
        let status =
            if message.contains("Cannot fit all items") || message.contains("Invalid input") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };

        (
            status,
            Json(json!({
                "error": message,
            })),
        )
            .into_response()
    }
}

async fn serve_ui() -> impl IntoResponse {
    // Read the UI file
    match std::fs::read_to_string("web/index.html") {
        Ok(html) => Html(html),
        Err(_) => Html(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Cutting Optimizer</title>
            </head>
            <body>
                <h1>Cutting Optimizer API</h1>
                <p>Web UI file not found. Please ensure web/index.html exists.</p>
                <h2>API Endpoints:</h2>
                <ul>
                    <li>GET /api/health - Health check</li>
                    <li>POST /api/optimize - Run optimization</li>
                    <li>POST /api/generate/svg - Generate SVG visualization</li>
                </ul>
            </body>
            </html>
        "#
            .to_string(),
        ),
    }
}

async fn serve_openapi_spec() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("Content-Type", "application/yaml")],
        OPENAPI_SPEC,
    )
}

async fn serve_swagger_ui() -> impl IntoResponse {
    Html(SWAGGER_UI_HTML)
}
