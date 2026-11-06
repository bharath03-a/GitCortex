import { afterEach, expect, it, vi } from "vitest";
import { createElement, type ReactNode } from "react";
import { cleanup, render } from "@testing-library/react";
import { readFileSync } from "node:fs";
const css = readFileSync("src/theme/tokens.css", "utf8");
import { CosmosCanvas } from "../../components/CosmosCanvas";

const captured = vi.hoisted<{ props: Record<string, unknown> }>(() => ({ props: {} }));
vi.mock("@cosmograph/react", () => ({
  CosmographProvider: ({ children }: { children: ReactNode }) => children,
  Cosmograph: (props: Record<string, unknown>) => {
    captured.props = props;
    return null;
  },
}));
afterEach(() => {
  cleanup();
  document.documentElement.style.removeProperty("--text-graph-label");
});

function block(selector: string): string {
  const match = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].find(
    (m) => m[1].trim() === selector || m[1].split(",").some((part) => part.trim() === selector),
  );
  expect(match, selector).toBeDefined();
  return match![2];
}
function declarations(source: string): Record<string, string> {
  return Object.fromEntries(
    [...source.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]),
  );
}
type RGB = [number, number, number];
function rgba(value: string): [number, number, number, number] {
  if (value.startsWith("#")) {
    expect(value).toMatch(/^#[\da-f]{6}$/i);
    return [
      parseInt(value.slice(1, 3), 16),
      parseInt(value.slice(3, 5), 16),
      parseInt(value.slice(5, 7), 16),
      1,
    ];
  }
  const match = value.match(/^rgba\(([^)]+)\)$/);
  expect(match, value).not.toBeNull();
  const parts = match![1].split(",").map(Number);
  expect(parts).toHaveLength(4);
  return parts as [number, number, number, number];
}
function over(foreground: string, background: RGB, opacity = 1): RGB {
  const [r, g, b, a] = rgba(foreground);
  return [r, g, b].map((v, i) => v * a * opacity + background[i] * (1 - a * opacity)) as RGB;
}
function luminance(rgb: RGB): number {
  return rgb
    .map((v) => v / 255)
    .map((v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4))
    .reduce((sum, v, i) => sum + v * [0.2126, 0.7152, 0.0722][i], 0);
}
function contrast(a: RGB, b: RGB): number {
  const values = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (values[0] + 0.05) / (values[1] + 0.05);
}

it("text_tokens_meet_aa_on_all_surfaces", () => {
  const base = declarations(block("@theme"));
  const panel = block(".graph-panel");
  const panelMatch = panel.match(/color-mix\(in srgb, var\((--[\w-]+)\) (\d+)%/);
  expect(panelMatch).not.toBeNull();
  // Foundation label and panel have no faded parent text. F7 removes legacy
  // per-component opacity utilities; F8 verifies the complete rendered DOM.
  const parentOpacity = Number(panel.match(/(?:^|;)\s*opacity:\s*([\d.]+)/)?.[1] ?? 1);
  const label = block(".graph-point-label");
  const labelOpacity = Number(label.match(/(?:^|;)\s*opacity:\s*([\d.]+)/)?.[1] ?? 1);
  const minima: Record<string, Record<string, number>> = {};
  for (const theme of ["light", "dark"]) {
    const tokens = { ...base, ...declarations(block(`[data-theme="${theme}"]`)) };
    minima[theme] = {};
    for (const name of [
      "text-primary",
      "text-muted",
      "text-dim",
      "accent-text",
      "warn",
      "bad",
      "good",
    ]) {
      let minimum = Infinity;
      for (const surface of ["void", "void-deep", "elevated", "elevated-hi"]) {
        const opaque = over(tokens[`--color-${surface}`], [0, 0, 0]);
        // Canvas layers: both grid directions, both major directions, glow.
        const layers = [...block(".canvas-bg").matchAll(/var\((--canvas-[\w-]+)\)/g)]
          .map((match) => match[1])
          .reverse();
        expect(layers).toHaveLength(5);
        const canvas = layers.reduce((bg, key) => over(tokens[key], bg), opaque);
        const backgrounds = [
          opaque,
          canvas,
          over(tokens[panelMatch![1]], canvas, Number(panelMatch![2]) / 100),
        ];
        for (const bg of backgrounds.flatMap((bg) => [
          bg,
          over(tokens["--color-accent-soft"], bg),
        ])) {
          const ratio = contrast(over(tokens[`--color-${name}`], bg, parentOpacity), bg);
          minimum = Math.min(minimum, ratio);
          expect.soft(ratio, `${theme}/${name}/${surface}`).toBeGreaterThanOrEqual(4.5);
        }
      }
      minima[theme][name] = Number(minimum.toFixed(2));
    }
    const labelBg = label.match(/background:\s*var\((--[\w-]+)\)/);
    expect(labelBg).not.toBeNull();
    const bg = over(tokens[labelBg![1]], [0, 0, 0]);
    expect(
      contrast(over(tokens["--color-text-primary"], bg, labelOpacity), bg),
    ).toBeGreaterThanOrEqual(4.5);
  }
  console.info("Foundation contrast minima:", JSON.stringify(minima));
});

it("type_scale_has_no_sub_12_values", () => {
  const tokens = declarations(block("@theme"));
  for (const [name, size] of Object.entries({ xs: 12, sm: 13, base: 14, lg: 16, title: 18 })) {
    expect(tokens[`--text-ui-${name}`]).toBe(`${size}px`);
  }
  expect(tokens["--text-graph-label"]).toBe("13px");
  for (const [name, value] of Object.entries(tokens).filter(([key]) => key.startsWith("--text-"))) {
    expect(parseFloat(value), name).toBeGreaterThanOrEqual(12);
  }
  expect(tokens).toMatchObject({
    "--leading-ui": "1.45",
    "--leading-heading": "1.25",
    "--font-weight-ui": "400",
    "--font-weight-label": "500",
    "--font-weight-heading": "600",
    "--tracking-ui": "0",
    "--tracking-heading": "0.02em",
  });
  expect(block("html,\nbody,\n#root")).toContain("font-size: 13px");
  expect(block("body,\n#root")).toContain("font-size: var(--text-ui-base)");
  expect(css).not.toContain("font-weight: 600 700");
  const label = block(".graph-point-label");
  expect(label).toContain("font-size: var(--text-graph-label)");
  expect(label).toContain("font-weight: var(--font-weight-label)");
  expect(label).toContain("line-height: 1.35");
  expect(label).toContain("letter-spacing: 0");
  expect(label).toContain("font-family: var(--font-mono)");
});

it("graph_label_size_clamps_to_12", async () => {
  const { readGraphLabelSize } = await import("../typography");
  for (const [value, expected] of [
    ["17px", 17],
    ["11px", 12],
    ["0px", 12],
    ["", 13],
    ["NaNpx", 13],
    ["Infinitypx", 13],
    ["garbage", 13],
    ["1rem", 13],
    ["13px junk", 13],
  ] as const) {
    document.documentElement.style.setProperty("--text-graph-label", value);
    expect(readGraphLabelSize(document.documentElement), value).toBe(expected);
  }
});

it("cosmograph_label_size_uses_css_token", () => {
  document.documentElement.style.setProperty("--text-graph-label", "17px");
  render(
    createElement(CosmosCanvas, {
      data: { nodes: [], edges: [] },
      selected: null,
      onSelect: vi.fn(),
      depth: 1,
      diffOverlay: null,
      unusedIds: null,
      godNodeIds: null,
      hotspotScores: null,
      complexityScores: null,
      boundaryNodeIds: null,
      boundaryEdgeKeys: null,
      theme: "light",
    }),
  );
  expect(captured.props).toMatchObject({
    pointLabelFontSize: 17,
    pointLabelClassName: "graph-point-label",
    hoveredPointLabelClassName: "graph-point-label",
    pointLabelWeightBy: "labelWeight",
    showDynamicLabels: true,
    showTopLabels: true,
    showHoveredPointLabel: true,
  });
});
