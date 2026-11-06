These fixtures were captured from the original `build_*` renderer bodies on
`origin/release/r1-combined-validation`, before extracting the renderers.
The store-loading boundary was replaced with fixed node and edge vectors so
IDs and row order are reproducible. The small graph includes the parsed
`tests/integration/fixtures/sample.rs` and a hostile, Unicode symbol name.
The large graph has 5,002 function nodes and stores SHA-256 digests instead
of full output. Legacy output must preserve these bytes and remain uncapped.

Run `cargo test -p gitcortex-viz --test export_golden` to compare. Intentional
format changes can be captured with `CAPTURE_GOLDENS=1`, after reviewing the
compatibility implications.
