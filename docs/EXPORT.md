# Graph Export Guide

`gcx export` turns an indexed branch—or a scoped neighborhood around a single symbol—into portable artifacts that work without the `gcx` binary or Kuzu store.

## Formats

| Format | Command | Typical consumer |
|---|---|---|
| `graph-json` | `gcx export --format graph-json --out graph.json` | Custom web UIs (d3-force, Cytoscape, vis-network, Cosmograph), CI tooling |
| `mermaid` | `gcx export --format mermaid --seed handle_request --direction up --depth 2` | GitHub PR comments, Markdown docs (default cap: 100 nodes) |
| `dot` | `gcx export --format dot --out graph.dot` | Graphviz (`dot -Tsvg`, `dot -Tpng`) |
| `svg` | `gcx export --format svg --out graph.svg` | Static docs and architecture diagrams |
| `graphml` | `gcx export --format graphml --out graph.graphml` | Gephi, yEd, Cytoscape |
| `cypher` | `gcx export --format cypher --out graph.cypher` | Neo4j (`cypher-shell`) |
| `html` | `gcx export --format html --out graph.html` | Standalone browser view (embeds graph JSON; loads vis-network from CDN) |
| `markdown` | `gcx export` | Human-readable `.gitcortex/context.md` codebase map |
| `json` | `gcx export --format json` | Legacy flat `{ branch, sha, symbols[], edges[] }` dump |

## Scoping a subgraph

Ask "what leads to X?" (`--direction up`), "what does X call?" (`--direction down`), or both:

```bash
# Callers leading into `verify_token` up to 2 hops away
gcx export --format graph-json \
  --seed verify_token \
  --direction up \
  --depth 2 \
  --edge-kinds calls \
  --out verify_token_callers.json
```

- **Exact seed resolution:** `--seed` matches by node ID first, then exact `qualified_name`, then exact `name`. If a short name matches multiple symbols, `gcx export` exits `2` and prints every candidate's `qualified_name` and ID—ambiguous names are never merged.
- **Traversal filtering:** `--edge-kinds` constrains every hop during BFS traversal, not just output filtering.
- **Deterministic truncation:** graphs exceeding `--max-nodes` (default `5000`, or `100` for `mermaid`) keep the seed and closest hops first, preserve deterministic `(file, start_line, id)` ordering, and mark `meta.truncated: true` (or emit `%% truncated: showing X of Y nodes` in Mermaid).

## Readiness and exit codes

Scoped graph exports (`graph-json`, `mermaid`, `dot`, `svg`, `graphml`, `cypher`, `html`) verify branch readiness before emitting output so a stale or missing index is never exported silently:

| Exit code | Meaning |
|---|---|
| `0` | Export succeeded |
| `1` | Operational error (for example, store open/corruption or I/O failure) |
| `2` | Invalid input (unknown/ambiguous `--seed`, invalid `--branch` ref, unknown `--edge-kinds`, or CLI usage error) |
| `3` | Index not ready (`missing`, `stale`, `unverified`, or `unavailable`) unless `--allow-stale` is passed |

Passing `--allow-stale` emits a warning to stderr, proceeds with the export, and records the observed state in `meta.readiness`.

## Versioned `graph-json` schema

Schema: [`docs/schemas/gitcortex-graph-v1.schema.json`](schemas/gitcortex-graph-v1.schema.json) (`format: "gitcortex.graph"`, `schema_version: 1`).

Load `graph.json` in a browser with `d3-force`:

```html
<svg width="960" height="600"></svg>
<script type="module">
import * as d3 from "https://cdn.jsdelivr.net/npm/d3@7/+esm";
const graph = await fetch("graph.json").then(r => r.json());
const svg = d3.select("svg");
const link = svg.selectAll("line").data(graph.edges).join("line").attr("stroke", "#666");
const node = svg.selectAll("circle").data(graph.nodes).join("circle").attr("r", 5).attr("fill", "#89b4fa");
node.append("title").text(d => `${d.qualified_name} (${d.kind}) ${d.file}:${d.start_line}`);
d3.forceSimulation(graph.nodes)
  .force("link", d3.forceLink(graph.edges).id(d => d.id).distance(60))
  .force("charge", d3.forceManyBody().strength(-120))
  .force("center", d3.forceCenter(480, 300))
  .on("tick", () => {
    link.attr("x1", d => d.source.x).attr("y1", d => d.source.y).attr("x2", d => d.target.x).attr("y2", d => d.target.y);
    node.attr("cx", d => d.x).attr("cy", d => d.y);
  });
</script>
```

## Mermaid in PRs and docs

Generate a bounded Mermaid diagram directly into your clipboard or a file:

```bash
gcx export --format mermaid \
  --seed src::auth::verify_token \
  --direction up \
  --depth 2 \
  --edge-kinds calls
```

Paste the output inside a ```` ```mermaid ```` fence in any GitHub PR comment or Markdown file. Hostile characters in symbol names (`"`, `<`, `>`, `|`, `` ` ``) are escaped automatically, and the seed node receives the `seed` style class.

## Privacy

Exports contain symbol metadata only: IDs, names, qualified names, kinds, file paths, line spans, visibility/async/unsafe flags, and typed relationships. **Source code text (function bodies, signatures, and doc comments) is never included in exported graphs.**
