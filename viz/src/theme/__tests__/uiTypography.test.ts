/// <reference types="vite/client" />
import ts from "typescript";
import { createElement } from "react";
import { render, screen, cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { FilterRail, type Visibility, type Flag } from "../../components/FilterRail";
import { Header } from "../../components/Header";
import { StatusBar } from "../../components/StatusBar";

const pendingF7b = new Set([
  "../../components/CompatibilityAtlas.tsx", // F7b
  "../../components/Inspector.tsx", // F7b
  "../../components/SearchPalette.tsx", // F7b
  "../../components/KeyboardHelp.tsx", // F7b
]);
const sources = Object.entries(
  import.meta.glob<string>("../../**/*.tsx", {
    query: "?raw",
    import: "default",
    eager: true,
  }),
).filter(([file]) => !/(__tests__|\.(test|spec)\.tsx$)/.test(file) && !pendingF7b.has(file));

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
          if (mono) {
            // Parse JSX attributes rather than stopping at a > inside an expression.
            // Mono leaves may contain identifiers, paths, branches, or command snippets.
            const parent = node.parent;
            if (ts.isJsxElement(parent)) {
              expect(
                parent.children.some(
                  (child) => ts.isJsxElement(child) || ts.isJsxSelfClosingElement(child),
                ),
                `${file}: mono container`,
              ).toBe(false);
            }
            const literal = ts.isJsxElement(parent)
              ? parent.children
                  .filter(ts.isJsxText)
                  .map((child) => child.text)
                  .join("")
                  .trim()
              : "";
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
