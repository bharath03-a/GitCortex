import { readFileSync } from "node:fs";
import { createElement } from "react";
import { render, screen, cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { FilterRail, type Visibility, type Flag } from "../../components/FilterRail";
import { StatusBar } from "../../components/StatusBar";

const files = [
  "App.tsx",
  ...["Header", "FilterRail", "StatusBar", "BranchPicker", "CosmosCanvas"].map(
    (name) => `components/${name}.tsx`,
  ),
];
const sources = files.map((file) => [file, readFileSync(`src/${file}`, "utf8")] as const);
afterEach(cleanup);

describe("F7a shell typography", () => {
  it("shell_has_no_sub_12px_text_classes", () => {
    for (const [file, source] of sources) {
      expect(source.match(/text-\[(?:[0-9]|1[01])px\]|\btext-(?:xs|sm)\b/g), file).toBeNull();
      // Disabled controls are the sole opacity exemption.
      expect(source.replaceAll("disabled:opacity-40", "").match(/\bopacity-\d+/g), file).toBeNull();
    }
  });
  it("ui_text_uses_type_tokens", () => {
    for (const [file, source] of sources) {
      expect(source.match(/text-\[\d+px\]/g), file).toBeNull();
      expect(source, file).toContain("text-ui-");
    }
  });
  it("section_titles_use_sans_without_wide_tracking", () => {
    for (const [file, source] of sources) {
      expect(source.match(/\buppercase\b|tracking-wider|tracking-\[/g), file).toBeNull();
    }
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
      expect(source.match(/<(?:footer|header|aside|kbd)[^>]*font-mono/g), file).toBeNull();
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
