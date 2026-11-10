# OpenOptimizer

<p align="center">
  <img src="assets/banner.png" alt="OpenOptimizer Banner" width="600">
</p>

Rust-based cutting optimizer that packs rectangular parts on sheet goods with guillotine cuts (straight through-cuts as on a panel saw). It exposes a reusable core library, an Axum HTTP API (with a lightweight web UI), and a CLI for batch processing.

## Highlights

- 🪚 **Saw-ready plans** – every layout can be cut with straight through-cuts; the response lists the cuts per stage.
- 📏 **Exact kerf** – every pair of parts is separated by the full kerf on every side (computed in 0.01 units, no rounding drift).
- 🧮 **Automatic format choice** – panels are filled one after another; for every panel all formats and packing methods (strip/knapsack packing in both directions, free-rectangle best fit) are compared, then improvement moves empty weak panels into offcuts and move panels to smaller formats.
- 🔁 **Deterministic search** – a multi-start search varies the tuning and keeps the plan with the least panel area, then the fewest panels; identical requests give identical results.
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

Layouts keep the panel orientation of the `panel_type`; cuts are planned in both directions.
Items with `can_rotate: false` therefore always keep their orientation relative to the panel
(grain direction).

### Kerf and cuts

`cut_width` is the saw kerf. Parts are kept at least one kerf apart on every side. Each
`PanelLayout` lists its `cuts` (guillotine cut tree, trimming cuts excluded):

```json
{ "stage": 1, "direction": "vertical", "x": 1849.0, "y": 15.0, "length": 1824.0 }
```

`stage` 1 cuts run through the whole trimmed panel, stage 2 cuts through a stage-1 strip, and so
on. `x`/`y` mark the start of the kerf.

### Search time

The first complete plan is always computed; further search rounds stop after `time_limit_ms`
(default 3000, maximum 30000). `statistics` in the response reports iterations, runtime, the
highest cut stage and a lower bound for the panel area (a plan whose `total_area` equals
the bound is optimal; the search stops early then).

### Unused areas

Each `PanelLayout` in the response includes an `unused_areas` array containing the offcuts of the
cutting plan, largest first. They do not overlap and each can be cut out with the listed cuts.
Offcuts smaller than 10 in either direction are omitted.

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

When `min_reusable_remnant_size` is set, every offcut with at least that area counts as reusable
remnant. The `actual_waste_*` fields reflect total waste minus that reusable area.

### Optional items

Each `PanelType` can include an `optional_items` array of filler pieces that the optimizer will
attempt to place **only when the effective waste exceeds 8%**. This is useful for producing spare
parts, test cuts, or small components that are nice-to-have but not required.

Optional items differ from regular items:
- They have no `quantity` – each entry represents a single piece.
- They include a `priority` field (higher values are tried first).
- They are only considered after all required items are placed and waste is above the threshold.
- They are placed into the offcuts of the finished plan, never on additional panels, and keep the
  plan guillotine-cuttable. An optional item may be placed several times.

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
4. Keep documentation (`README.md`, `openapi.yaml`, `.github/copilot-instructions.md`) in sync with any schema or workflow updates.
5. For algorithm changes compare total panel area, panel count, kerf/overlap validity and guillotine cuttability on a fixed set of real requests before and after the change.

More contributor guidance lives in `.github/copilot-instructions.md`.
