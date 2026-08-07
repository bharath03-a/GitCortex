// Materializes synthetic graph fixtures for manual scale profiling in Chrome
// DevTools. Not run in CI or unit tests — those exercise generateSyntheticGraph
// directly at small sizes. Run via `npm run bench:fixtures`.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { generateSyntheticGraph } from "../src/graph/fixtures";

const SCALES = [1_000, 10_000, 50_000, 100_000];
// This script is bundled and run from a temp location, so it can't derive
// the output path from import.meta.url — use the npm-script working
// directory (the viz/ package root) instead.
const OUT_DIR = join(process.cwd(), "bench-fixtures");

mkdirSync(OUT_DIR, { recursive: true });

for (const nodeCount of SCALES) {
  const graph = generateSyntheticGraph({ nodeCount, seed: 1 });
  const path = join(OUT_DIR, `graph-${nodeCount}.json`);
  writeFileSync(path, JSON.stringify(graph));
  const bytes = JSON.stringify(graph).length;
  console.log(
    `${path}: ${graph.nodes.length} nodes, ${graph.edges.length} edges, ${(bytes / 1024 / 1024).toFixed(2)} MB`,
  );
}
