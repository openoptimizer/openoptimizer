# OpenOptimizer

Rust-based cutting optimizer that packs rectangular parts on sheet goods using a best-fit decreasing heuristic. It exposes a reusable core library, an Axum HTTP API (with a lightweight web UI), and a CLI for batch processing.

## Highlights

- ✅ **Single, predictable heuristic** – always uses Best Fit Decreasing with optional knobs for shelf-first or remnant-friendly packing.
- 📦 **Three curated examples** – `examples/simple.yaml`, `examples/complex.json`, and `examples/furniture.yaml` cover the most common workloads.
- 🌐 **Documented API** – OpenAPI spec lives in `openapi.yaml`; served endpoints power the demo UI and any custom integrations.
- 🧩 **Composable crates** – `optimizer-core`, `optimizer-api`, and `optimizer-cli` share the same logic for cross-validation.

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
