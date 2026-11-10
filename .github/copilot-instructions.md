# Copilot Instructions

These notes keep AI-assisted contributions consistent with the current vision of OpenOptimizer.

## Scope & Architecture

- Every layout must be guillotine-cuttable (panel saw). All packers split free rectangles with straight through-cuts; `optimizer/guillotine.rs` recovers the cut tree and offcuts. Never add non-guillotine placement (e.g. maximal-rectangles) packers.
- Geometry is computed in integer units of 0.01 (`optimizer/model.rs`). Pieces are inflated by the kerf on the right/top and usable panel areas by one kerf, so the full kerf holds on every side. Do not compare `f64` coordinates for packing decisions.
- The search (`optimizer/search.rs`) fills panels one by one, compares all panel types and packing methods per panel, applies improvement moves and runs a deterministic multi-start (seeded RNG). Objective: total panel area, then number of panels, then concentrated waste. Keep results reproducible for identical requests.
- Layouts keep the panel orientation of the request; `can_rotate: false` refers to that orientation.
- Shared types live in `crates/optimizer-core/src/types.rs`. Any schema change must be reflected in `openapi.yaml`, the CLI, and the web UI payload builders.
- Optional items are strictly treated as waste-reduction fillers; never make them mandatory inputs.
- Optional items use the `OptionalItem` type (not `Item`) with a `priority` field (no `quantity`). They are only considered when effective waste exceeds 8%.

## Coding Standards

- Prefer small, focused modules like `optimizer/packer.rs`, `optimizer/search.rs`, `optimizer/guillotine.rs`, `optimizer/optional.rs`, and `optimizer/summary.rs`.
- Add short doc comments when logic is not self-evident. Avoid noisy or restating comments.
- Keep everything in ASCII and default Rust formatting via `cargo fmt`.
- Use descriptive test names inside `optimizer::tests` and keep the total count lean but meaningful.

## Tests & Validation

1. `cargo fmt`
2. `cargo clippy --workspace --all-targets` (when touching Rust logic)
3. `cargo test -p optimizer-core`
4. `cargo test` (workspace-wide) before release branches
5. `docker build --target api -t openoptimizer-api:latest .` for API deployments

Document any non-obvious test coverage gaps directly in PR descriptions.

Algorithm changes additionally need a before/after comparison on a fixed benchmark of real requests (total panel area, panel count, kerf/overlap/count validity, guillotine cuttability, runtime). Never accept a change that produces invalid layouts to gain material.

## Examples & Docs

- Keep exactly **three** example payloads inside `examples/` (`simple.yaml`, `complex.json`, `furniture.yaml`). Add new scenarios elsewhere (e.g., gists) if needed.
- Every meaningful change requires updates to `README.md`, `openapi.yaml`, and this file if the guidance changes.
- Surface API behavior changes in both the README and the OpenAPI spec.

## Pull Request Checklist

- [ ] New/changed behavior covered by tests or rationale for omission provided.
- [ ] Examples still run with `optimizer-cli` and via `POST /api/optimize`.
- [ ] Documentation (+ OpenAPI) updated.
- [ ] `cargo fmt` and `cargo test` (at least the touched crates) executed.
- [ ] Copilot suggestions reviewed for hard-coded secrets, UUIDs, or user data before committing.
