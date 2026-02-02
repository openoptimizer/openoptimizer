# OpenOptimizer

<p align="center">
  <img src="assets/banner.png" alt="OpenOptimizer Banner" width="600">
</p>

Rust-based cutting optimizer that packs rectangular parts on sheet goods using a best-fit decreasing heuristic. It exposes a reusable core library, an Axum HTTP API (with a lightweight web UI), and a CLI for batch processing.

## Highlights

- ✅ **Single, predictable heuristic** – always uses Best Fit Decreasing with optional knobs for shelf-first or remnant-friendly packing.
- 📦 **Three curated examples** – `examples/simple.yaml`, `examples/complex.json`, and `examples/furniture.yaml` cover the most common workloads.
- 🌐 **Documented API** – OpenAPI spec lives in `openapi.yaml`; served endpoints power the demo UI and any custom integrations.
- 🧩 **Composable crates** – `optimizer-core`, `optimizer-api`, and `optimizer-cli` share the same logic for cross-validation.
- 📐 **Unused areas** – Each panel layout includes rectangular leftover regions, enabling downstream tools to visualize or reuse remnants.

## Repository Layout

| Path | Description |
| --- | --- |
| `crates/optimizer-core` | Core packing engine and request/response types |
| `crates/optimizer-api` | Axum server exposing `/api/optimize`, `/api/health`, `/api/generate/svg` |
| `crates/optimizer-cli` | CLI wrapper for running optimizations from the terminal |
| `web/` | Static HTML/JS UI served by the API |
| `examples/` | Three ready-to-run sample requests |
| `openapi.yaml` | Machine-readable API contract |

## Getting Started

### Prerequisites

- Rust 1.74+ (for workspace builds)
- Docker (optional, for containerized API)

### Run the CLI

```cmd
cargo run -p optimizer-cli -- examples\simple.yaml
```

The CLI accepts JSON or YAML payloads matching the schema in `openapi.yaml`.

### Run the API locally

```cmd
cargo run -p optimizer-api
```

Visit `http://localhost:3000` for the embedded UI. The API container also ships with:

- `http://localhost:3000/openapi.yaml` – raw OpenAPI 3.1 spec (same as `/openapi.yaml` in the repo)
- `http://localhost:3000/docs` – Swagger UI viewer backed by that spec

Callers can use the OpenAPI contract via tools such as [Hoppscotch](https://hoppscotch.io/) or `curl`:

```cmd
curl -s http://localhost:3000/api/health
curl -s -X POST http://localhost:3000/api/optimize ^
  -H "Content-Type: application/json" ^
  -d @examples/complex.json
```

### Run inside Docker

```cmd
docker build --target api -t openoptimizer-api:latest .
docker run -d -p 3000:3000 openoptimizer-api:latest
```

Prefer skipping local builds? Pull the published images directly from Docker Hub:

```cmd
docker pull openoptimizer/openoptimizer-api:latest
docker run -d -p 3000:3000 openoptimizer/openoptimizer-api:latest

docker pull openoptimizer/openoptimizer-cli:latest
docker run --rm -v %CD%\examples:/examples openoptimizer/openoptimizer-cli:latest examples/simple.yaml
```

Replace `latest` with a tagged version (e.g., `0.0.2`) for reproducible deployments.

### Panel trimming

If a panel needs to be cleaned up along its edges before useful cuts begin, add a `trimming`
value to that `panel_type`. The optimizer will trim that amount from every side (reducing the
usable width/height by twice the trimming value) before placing any parts, and the trimmed border
is reported as waste in the summary.

```yaml
panel_types:
  - id: "plywood_8x4"
    width: 2440.0
    height: 1220.0
    trimming: 6.0
```

Omit the field (or set it to 0) to use the full panel.

Panels may be used in either orientation (width/height swapped) to improve packing. When a panel
is rotated, the resulting `PanelLayout.width` and `PanelLayout.height` reflect the orientation
chosen by the optimizer.

### Unused areas

Each `PanelLayout` in the response includes an `unused_areas` array containing the rectangular
leftover regions on that panel after all placements. These areas are guaranteed to be rectangles
(4 edges); when the free space forms an irregular shape, the optimizer splits it and keeps the
rectangle with the largest area (square meters).

Example response snippet:

```json
{
  "panel_type_id": "plywood_8x4",
  "panel_number": 1,
  "placements": [ ... ],
  "unused_areas": [
    { "x": 1200.0, "y": 0.0, "width": 1240.0, "height": 1220.0 },
    { "x": 0.0, "y": 600.0, "width": 1200.0, "height": 620.0 }
  ]
}
```

Use these rectangles to visualize waste, plan reusable offcuts, or feed into downstream nesting.

### Optional items

Each `PanelType` can include an `optional_items` array of filler pieces that the optimizer will
attempt to place **only when the effective waste exceeds 8%**. This is useful for producing spare
parts, test cuts, or small components that are nice-to-have but not required.

Optional items differ from regular items:
- They have no `quantity` – each entry represents a single piece.
- They include a `priority` field (higher values are tried first).
- They are only considered after all required items are placed and waste is above the threshold.

```yaml
panel_types:
  - id: "plywood_8x4"
    width: 2440.0
    height: 1220.0
    optional_items:
      - id: "spare_shelf"
        width: 300.0
        height: 200.0
        can_rotate: true
        priority: 10
      - id: "test_piece"
        width: 100.0
        height: 100.0
        can_rotate: true
        priority: 5
```

The `optional_items_used` array in the response lists which optional items were successfully placed.

## Example Payloads

| File | Format | Purpose |
| --- | --- | --- |
| `examples/simple.yaml` | YAML | Small job showcasing defaults |
| `examples/complex.json` | JSON | Larger cabinet build with mixed parts |
| `examples/furniture.yaml` | YAML | Multi-project sheet layout emphasizing rotation rules |

Feel free to copy one of these files as a base for your own requests.

## API Contract

`openapi.yaml` documents all request/response schemas plus error envelopes. Import it into Swagger UI, Postman, or Insomnia for interactive exploration. Key paths:

- `GET /api/health` – status and version metadata
- `POST /api/optimize` – returns packed layouts (`OptimizationResult`)
- `POST /api/generate/svg` – renders SVG markup for a previously computed result

## Development Workflow

1. Format code with `cargo fmt`.
2. Run tests via `cargo test -p optimizer-core` (and `cargo test` for a full sweep when changing shared types).
3. If API behavior changes, rebuild the Docker image and smoke-test the `/api/optimize` endpoint.
4. Keep documentation (`README.md`, `openapi.yaml`, `COPILOT.md`) in sync with any schema or workflow updates.

More contributor guidance lives in `COPILOT.md`.
