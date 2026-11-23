/// <reference types="vite/client" />
import ts from "typescript";
import { createElement } from "react";
import { render, screen, cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { CompatibilityAtlas } from "../../components/CompatibilityAtlas";
import { FilterRail, type Visibility, type Flag } from "../../components/FilterRail";
import { Header } from "../../components/Header";
import { Inspector } from "../../components/Inspector";
import { KeyboardHelp } from "../../components/KeyboardHelp";
import { SearchPalette } from "../../components/SearchPalette";
import { StatusBar } from "../../components/StatusBar";
import type { GraphData, RawNode } from "../../api";

const sources = Object.entries(
  import.meta.glob<string>("../../**/*.tsx", {
    query: "?raw",
    import: "default",
    eager: true,
  }),
).filter(([file]) => !/(__tests__|\.(test|spec)\.tsx$)/.test(file));

function violations(source: string): string[] {
  return [
    ...[...source.matchAll(/text-\[([\d.]+)(px|rem|em)\]/g)]
      .filter(([, size, unit]) => Number(size) * (unit === "px" ? 1 : 13) < 12)
      .map(([match]) => match),
    ...(source.match(/\btext-xs\b|\bfontSize\b/g) ?? []),
    ...(source
      .replace(/disabled:opacity-(?:[\w.]+|\[[^\]]+\])/g, "")
      .match(
        /\bopacity-(?:[\w.]+|\[[^\]]+\])|text-(?:\([^)]*\)|\[[^\]]*\]|[\w-]+)\/(?:[\d.]+|\[[^\]]+\])/g,
      ) ?? []),
  ];
}
afterEach(cleanup);

describe("F7a shell typography", () => {
  it("detects small and faded text while exempting disabled controls", () => {
    for (const value of [
      "text-[11px]",
      "text-[0.8rem]",
      "text-[.8em]",
      "text-xs",
      "fontSize: 11",
      "opacity-50",
      "text-red-500/50",
      "text-(--color-text-muted)/60",
    ]) {
      expect(violations(value), value).not.toEqual([]);
    }
    expect(violations("disabled:opacity-50 text-[12px] text-[1rem]")).toEqual([]);
  });
  it("shell_has_no_sub_12px_text_classes", () => {
    for (const [file, source] of sources) {
      expect(violations(source), file).toEqual([]);
    }
  });
  it("ui_text_uses_type_tokens", () => {
    for (const [file, source] of sources) {
      expect(source.match(/text-\[\d+px\]/g), file).toBeNull();
    }
  });
  it("section_titles_use_sans_without_wide_tracking", () => {
    for (const [file, source] of sources) {
      expect(source.match(/\buppercase\b|tracking-wider|tracking-\[/g), file).toBeNull();
    }
  });
  it("search shrinks in the header and uses body-sized prompt text", () => {
    const noop = () => {};
    render(
      createElement(Header, {
        onSearch: noop,
        onShowHelp: noop,
        repoName: "GitCortex",
        activeBranch: "main",
        onSetActiveBranch: noop,
        diffHead: null,
        onSetDiffHead: noop,
        theme: "light",
        onToggleTheme: noop,
      }),
    );
    const prompt = screen.getByText("Find a symbol, file, or qualified name");
    expect(prompt.className).toContain("text-ui-base");
    const classes = prompt.closest("button")!.className.split(/\s+/);
    expect(classes).toEqual(
      expect.arrayContaining(["min-w-0", "flex-1", "basis-[240px]", "max-w-[620px]"]),
    );
    expect(classes).not.toContain("w-full");
  });
  it("renders readable rail headings, descriptions, and counters", () => {
    const noop = () => {};
    render(
      createElement(FilterRail, {
        data: null,
        visibleNodeCount: 0,
        hotFiles: null,
        density: "connected",
        onDensityChange: noop,
        viewMode: "atlas",
        onViewModeChange: noop,
        canInvestigate: false,
        insightLens: null,
        onInsightLensChange: noop,
        hiddenKinds: new Set<string>(),
        setHiddenKinds: noop,
        hiddenEdgeKinds: new Set<string>(),
        setHiddenEdgeKinds: noop,
        hiddenConfidence: new Set<string>(),
        setHiddenConfidence: noop,
        hiddenVisibility: new Set<Visibility>(),
        setHiddenVisibility: noop,
        flagFilter: new Set<Flag>(),
        setFlagFilter: noop,
        onCollapse: noop,
      }),
    );
    for (const title of ["Workspace", "Graph scope", "Insights", "Advanced filters"]) {
      expect(screen.getByText(title).className).toContain("text-ui-xs");
      expect(screen.getByText(title).className).toContain("font-heading");
    }
    expect(screen.getByText("Frequently changed files and relations").className).toContain(
      "text-ui-base",
    );
    expect(screen.getByText("0 visible · 0 loaded").className).toContain("text-ui-xs");
  });
  it("mono_is_scoped_to_identifiers_and_paths", () => {
    for (const [file, source] of sources) {
      const ast = ts.createSourceFile(
        file,
        source,
        ts.ScriptTarget.Latest,
        true,
        ts.ScriptKind.TSX,
      );
      function visit(node: ts.Node) {
        if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) {
          const mono = node.attributes.properties.some(
            (attr) =>
              ts.isJsxAttribute(attr) &&
              attr.name.getText(ast) === "className" &&
              /\bfont-mono\b/.test(attr.initializer?.getText(ast) ?? ""),
          );
          if (mono && ts.isJsxOpeningElement(node) && ts.isJsxElement(node.parent)) {
            // Parse JSX attributes rather than stopping at a > inside an expression.
            // Mono leaves may contain identifiers, paths, branches, or command snippets.
            const parent = node.parent;
            expect(
              parent.children.some(
                (child) => ts.isJsxElement(child) || ts.isJsxSelfClosingElement(child),
              ),
              `${file}: mono container`,
            ).toBe(false);
            const literal = parent.children
              .filter(ts.isJsxText)
              .map((child) => child.text)
              .join("")
              .trim();
            expect(literal, `${file}: mono prose`).toMatch(/^(?:gcx (?:hook|init)|vs)?$/);
          }
        }
        ts.forEachChild(node, visit);
      }
      visit(ast);
    }
    render(
      createElement(StatusBar, {
        data: null,
        selected: null,
        activeBranch: "main",
        lastSha: null,
        diffOverlay: null,
      }),
    );
    expect(screen.getByText("main").className).toContain("font-mono");
    expect(screen.getByText("Click a node or press / to investigate").className).not.toContain(
      "font-mono",
    );
    expect(screen.getByRole("contentinfo").className).toContain("text-ui-xs");
  });
});

const SAMPLE_NODE: RawNode = {
  id: "n1",
  name: "compute_blast_radius",
  kind: "function",
  file: "crates/gitcortex-cli/src/cmd/query/blast_radius.rs",
  start_line: 42,
  end_line: 118,
  qualified_name: "gitcortex::cmd::query::blast_radius::compute_blast_radius",
  loc: 77,
  visibility: "pub",
  is_async: true,
  is_unsafe: false,
};

const CALLER_NODE: RawNode = {
  id: "n2",
  name: "run_query_command",
  kind: "function",
  file: "crates/gitcortex-cli/src/cmd/query.rs",
  start_line: 10,
  end_line: 38,
  qualified_name: "gitcortex::cmd::query::run_query_command",
  loc: 29,
  visibility: "pub_crate",
  is_async: false,
  is_unsafe: false,
};

const SAMPLE_GRAPH: GraphData = {
  nodes: [SAMPLE_NODE, CALLER_NODE],
  edges: [{ src: CALLER_NODE.id, dst: SAMPLE_NODE.id, kind: "calls" }],
};

describe("F7b detail panel typography", () => {
  it("renders inspector metadata, paths, and status tones with type tokens", () => {
    const noop = () => {};
    render(
      createElement(Inspector, {
        node: SAMPLE_NODE,
        data: SAMPLE_GRAPH,
        onClose: noop,
        onSelect: noop,
        depth: 2,
        onDepthChange: noop,
        branch: "main",
        hotspot: {
          path: SAMPLE_NODE.file,
          touches: 5,
          additions: 120,
          deletions: 14,
          last_changed: 1_700_000_000,
        },
      }),
    );
    const title = screen.getByRole("heading", { name: "compute_blast_radius" });
    expect(title.className).toContain("text-ui-base");
    expect(title.className).toContain("font-mono");

    const fileLinkText = screen.getByText(/blast_radius\.rs:42/);
    expect(fileLinkText.className).toContain("font-mono");
    expect(fileLinkText.className).toContain("text-ui-xs");

    const asyncBadge = screen.getByText("async");
    expect(asyncBadge.className).toContain("text-ui-xs");
    expect(asyncBadge.className).toContain("text-(--color-accent-text)");
    expect(asyncBadge.className).not.toContain("font-mono");

    const callerBtn = screen.getByText("run_query_command");
    expect(callerBtn.className).toContain("font-mono");
    expect(callerBtn.className).toContain("text-ui-sm");
  });

  it("renders search palette input, results, and paths with readable tokens", () => {
    const noop = () => {};
    render(
      createElement(SearchPalette, {
        data: SAMPLE_GRAPH,
        onClose: noop,
        onSelect: noop,
      }),
    );
    const input = screen.getByRole("combobox");
    expect(input.className).toContain("text-ui-base");

    const esc = screen.getByText("esc");
    expect(esc.className).toContain("text-ui-xs");
    expect(esc.className).not.toContain("font-mono");

    const symbol = screen.getByText("compute_blast_radius");
    expect(symbol.className).toContain("font-mono");
    expect(symbol.className).toContain("text-ui-sm");
  });

  it("renders compatibility atlas prose, metrics, and symbol rows without blanket mono", () => {
    const noop = () => {};
    render(
      createElement(CompatibilityAtlas, {
        data: SAMPLE_GRAPH,
        selected: SAMPLE_NODE,
        onSelect: noop,
        onSearch: noop,
        onUseWebGl: noop,
        variant: "compatibility",
      }),
    );
    const body = screen.getByText(/WebGL acceleration is unavailable/);
    expect(body.className).toContain("text-ui-base");
    expect(body.className).not.toContain("font-mono");

    const heading = screen.getByText("Repository structure");
    expect(heading.className).toContain("text-ui-xs");
    expect(heading.className).toContain("font-heading");
    expect(heading.className).not.toContain("font-mono");

    const selectedSymbol = screen.getByText("compute_blast_radius");
    expect(selectedSymbol.className).toContain("font-mono");
    expect(selectedSymbol.className).toContain("text-ui-sm");
    expect(selectedSymbol.closest("button")!.className).toContain("text-(--color-accent-text)");
  });

  it("renders keyboard help descriptions at body size and keycaps at >=12px", () => {
    render(createElement(KeyboardHelp, { onClose: () => {} }));
    const desc = screen.getByText("Show this help");
    expect(desc.className).toContain("text-ui-base");
    const key = screen.getByText("?");
    expect(key.className).toContain("text-ui-xs");
    expect(key.className).toContain("font-mono");
  });
});
