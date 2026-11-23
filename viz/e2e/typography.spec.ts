import { expect, test, type Page } from "@playwright/test";
import { mkdirSync } from "node:fs";
import fixture from "./fixtures/typography-graph.json" with { type: "json" };

interface TypographyViolation {
  tag: string;
  text: string;
  reason: string;
  fontSize: number;
  fontWeight: string;
  fontFamily: string;
  contrast?: number;
}

interface LabelMeasurement {
  text: string;
  fontSize: number;
  fontWeight: string;
  fontFamily: string;
  bgAlpha: number;
  contrast: number;
}

type FixtureMode = "populated" | "empty" | "error" | "loading";

async function mockGraphApi(page: Page, mode: FixtureMode = "populated") {
  await page.route("**/api/branches", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        active: fixture.branch,
        last_sha: fixture.last_sha,
        branches: fixture.branches,
      }),
    });
  });

  await page.route("**/api/graph/manifest*", async (route) => {
    if (mode === "error") {
      await route.fulfill({
        status: 500,
        contentType: "application/json",
        body: JSON.stringify({ error: "Simulated graph store failure" }),
      });
      return;
    }
    if (mode === "loading") {
      await new Promise((resolve) => setTimeout(resolve, 15_000));
      await route.abort();
      return;
    }
    const nodes = mode === "empty" ? [] : fixture.nodes;
    const edges = mode === "empty" ? [] : fixture.edges;
    const byKind = new Map<string, number>();
    for (const n of nodes) byKind.set(n.kind, (byKind.get(n.kind) ?? 0) + 1);
    const byEdge = new Map<string, number>();
    for (const e of edges) byEdge.set(e.kind, (byEdge.get(e.kind) ?? 0) + 1);

    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        branch: fixture.branch,
        repo_name: fixture.repo_name,
        snapshot: fixture.last_sha,
        total_nodes: nodes.length,
        total_edges: edges.length,
        nodes_by_kind: [...byKind.entries()],
        edges_by_kind: [...byEdge.entries()],
        recommended_chunk: 500,
        max_chunk: 1000,
      }),
    });
  });

  await page.route("**/api/graph/nodes*", async (route) => {
    const nodes = mode === "empty" ? [] : fixture.nodes;
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        branch: fixture.branch,
        snapshot: fixture.last_sha,
        offset: 0,
        count: nodes.length,
        next_offset: null,
        nodes,
      }),
    });
  });

  await page.route("**/api/graph/edges*", async (route) => {
    const edges = mode === "empty" ? [] : fixture.edges;
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        branch: fixture.branch,
        snapshot: fixture.last_sha,
        offset: 0,
        count: edges.length,
        next_offset: null,
        edges,
      }),
    });
  });

  await page.route("**/api/neighborhood/*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        nodes: fixture.nodes,
        edges: fixture.edges,
        limit_reached: false,
      }),
    });
  });

  await page.route("**/api/callers-by-id/*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        target: fixture.nodes[0].id,
        depth: 2,
        risk_level: "HIGH",
        truncated: false,
        hops: [{ hop: 1, nodes: [fixture.nodes[1], fixture.nodes[7]] }],
      }),
    });
  });

  await page.route("**/api/history/hot-files*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        branch: fixture.branch,
        scanned_commits: 42,
        files: fixture.hot_files,
      }),
    });
  });

  await page.route("**/api/unused*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        branch: fixture.branch,
        total: 1,
        symbols: [fixture.nodes[9]],
      }),
    });
  });

  await page.route("**/api/god_nodes*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        branch: fixture.branch,
        total: 1,
        nodes: [fixture.nodes[0]],
      }),
    });
  });
}

async function setTheme(page: Page, theme: "light" | "dark") {
  const current = await page.evaluate(() => document.documentElement.dataset.theme ?? "light");
  if (current !== theme) {
    const label = theme === "dark" ? "Use dark theme" : "Use light theme";
    await page.getByRole("button", { name: label }).click();
    await expect
      .poll(() => page.evaluate(() => document.documentElement.dataset.theme))
      .toBe(theme);
  }
}

async function ensureWebGlMode(page: Page) {
  const retryBtn = page.getByRole("button", { name: /Retry WebGL|Open symbol graph/i });
  if (await retryBtn.isVisible().catch(() => false)) {
    await retryBtn.click();
  }
  await expect(page.locator("canvas").first()).toBeVisible({ timeout: 15_000 });
}

async function auditVisibleTypography(page: Page): Promise<{
  checkedCount: number;
  violations: TypographyViolation[];
}> {
  return page.evaluate(async () => {
    const finiteAnims = document.getAnimations().filter((a) => {
      const timing = a.effect?.getComputedTiming();
      return timing && timing.iterations !== Infinity;
    });
    await Promise.allSettled(finiteAnims.map((a) => a.finished));

    interface Rgba {
      r: number;
      g: number;
      b: number;
      a: number;
    }

    const probeCanvas = document.createElement("canvas");
    probeCanvas.width = 1;
    probeCanvas.height = 1;
    const probeCtx = probeCanvas.getContext("2d", { willReadFrequently: true });

    function parseCssColor(value: string): Rgba {
      if (!value || value === "transparent" || value === "rgba(0, 0, 0, 0)") {
        return { r: 0, g: 0, b: 0, a: 0 };
      }
      if (probeCtx) {
        probeCtx.clearRect(0, 0, 1, 1);
        probeCtx.fillStyle = "rgba(0, 0, 0, 0)";
        probeCtx.fillStyle = value;
        probeCtx.fillRect(0, 0, 1, 1);
        const [r, g, b, a] = probeCtx.getImageData(0, 0, 1, 1).data;
        return { r, g, b, a: a / 255 };
      }
      const m = value.match(
        /rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:[,/\s]+([\d.]+))?\s*\)/i,
      );
      if (!m) return { r: 0, g: 0, b: 0, a: 0 };
      return {
        r: Number(m[1]),
        g: Number(m[2]),
        b: Number(m[3]),
        a: m[4] === undefined ? 1 : Number(m[4]),
      };
    }

    function composite(fg: Rgba, bg: Rgba): Rgba {
      const a = fg.a + bg.a * (1 - fg.a);
      if (a <= 0) return { r: 0, g: 0, b: 0, a: 0 };
      return {
        r: (fg.r * fg.a + bg.r * bg.a * (1 - fg.a)) / a,
        g: (fg.g * fg.a + bg.g * bg.a * (1 - fg.a)) / a,
        b: (fg.b * fg.a + bg.b * bg.a * (1 - fg.a)) / a,
        a,
      };
    }

    function relLum(c: Rgba): number {
      const ch = [c.r, c.g, c.b].map((v) => {
        const s = v / 255;
        return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
      });
      return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
    }

    function contrastRatio(fg: Rgba, bg: Rgba): number {
      const l1 = relLum(fg);
      const l2 = relLum(bg);
      return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
    }

    function effectiveBackground(el: Element): Rgba {
      const chain: Element[] = [];
      let cur: Element | null = el;
      while (cur) {
        chain.push(cur);
        cur = cur.parentElement;
      }
      chain.reverse();
      let bg: Rgba = { r: 255, g: 255, b: 255, a: 1 };
      for (const node of chain) {
        const style = getComputedStyle(node);
        const layer = parseCssColor(style.backgroundColor);
        const op = Number(style.opacity);
        if (layer.a > 0) {
          bg = composite({ ...layer, a: layer.a * (Number.isFinite(op) ? op : 1) }, bg);
        }
      }
      return bg;
    }

    function isElementVisible(el: Element): boolean {
      const style = getComputedStyle(el);
      if (
        style.display === "none" ||
        style.visibility === "hidden" ||
        Number(style.opacity) === 0
      ) {
        return false;
      }
      const rect = el.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0;
    }

    function isDisabled(el: Element): boolean {
      return Boolean(el.closest("button:disabled, input:disabled, [aria-disabled='true']"));
    }

    const violations: TypographyViolation[] = [];
    let checkedCount = 0;

    const all = document.querySelectorAll("body *");
    for (const el of all) {
      if (el.closest("svg, script, style, noscript")) continue;
      if (!isElementVisible(el)) continue;

      const directText = [...el.childNodes]
        .filter((n) => n.nodeType === Node.TEXT_NODE)
        .map((n) => n.textContent ?? "")
        .join("")
        .trim();
      if (!directText) continue;

      checkedCount += 1;
      const style = getComputedStyle(el);
      const fontSize = parseFloat(style.fontSize);
      const fontWeight = style.fontWeight;
      const fontFamily = style.fontFamily;
      const tag = el.tagName.toLowerCase();

      if (fontSize < 12 - 0.01) {
        violations.push({
          tag,
          text: directText.slice(0, 60),
          reason: `fontSize ${fontSize}px < 12px`,
          fontSize,
          fontWeight,
          fontFamily,
        });
      }

      if (tag === "p" && fontSize < 14 - 0.01) {
        violations.push({
          tag,
          text: directText.slice(0, 60),
          reason: `prose <p> fontSize ${fontSize}px < 14px`,
          fontSize,
          fontWeight,
          fontFamily,
        });
      }

      if (!isDisabled(el)) {
        const bg = effectiveBackground(el);
        const fgRaw = parseCssColor(style.color);
        let opacityFactor = 1;
        let cur: Element | null = el;
        while (cur) {
          const op = Number(getComputedStyle(cur).opacity);
          if (Number.isFinite(op)) opacityFactor *= op;
          cur = cur.parentElement;
        }
        const fg = composite({ ...fgRaw, a: fgRaw.a * opacityFactor }, bg);
        const ratio = contrastRatio(fg, bg);
        if (ratio < 4.49) {
          violations.push({
            tag,
            text: directText.slice(0, 60),
            reason: `contrast ${ratio.toFixed(2)} < 4.5`,
            fontSize,
            fontWeight,
            fontFamily,
            contrast: Number(ratio.toFixed(2)),
          });
        }
      }
    }

    return { checkedCount, violations };
  });
}

async function measureVisibleGraphLabels(page: Page): Promise<LabelMeasurement[]> {
  return page.evaluate(() => {
    interface Rgba {
      r: number;
      g: number;
      b: number;
      a: number;
    }

    function parseCssColor(value: string): Rgba {
      const m = value.match(
        /rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:[,/\s]+([\d.]+))?\s*\)/i,
      );
      if (!m) return { r: 0, g: 0, b: 0, a: 0 };
      return {
        r: Number(m[1]),
        g: Number(m[2]),
        b: Number(m[3]),
        a: m[4] === undefined ? 1 : Number(m[4]),
      };
    }

    function composite(fg: Rgba, bg: Rgba): Rgba {
      const a = fg.a + bg.a * (1 - fg.a);
      if (a <= 0) return { r: 0, g: 0, b: 0, a: 0 };
      return {
        r: (fg.r * fg.a + bg.r * bg.a * (1 - fg.a)) / a,
        g: (fg.g * fg.a + bg.g * bg.a * (1 - fg.a)) / a,
        b: (fg.b * fg.a + bg.b * bg.a * (1 - fg.a)) / a,
        a,
      };
    }

    function relLum(c: Rgba): number {
      const ch = [c.r, c.g, c.b].map((v) => {
        const s = v / 255;
        return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
      });
      return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
    }

    function contrastRatio(fg: Rgba, bg: Rgba): number {
      const l1 = relLum(fg);
      const l2 = relLum(bg);
      return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
    }

    const labels = [...document.querySelectorAll(".graph-point-label")];
    const results: LabelMeasurement[] = [];
    for (const el of labels) {
      const style = getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      const text = (el.textContent ?? "").trim();
      if (
        !text ||
        style.display === "none" ||
        style.visibility === "hidden" ||
        Number(style.opacity) === 0 ||
        rect.width <= 0 ||
        rect.height <= 0
      ) {
        continue;
      }
      const bg = parseCssColor(style.backgroundColor);
      const fg = composite(parseCssColor(style.color), bg);
      results.push({
        text,
        fontSize: parseFloat(style.fontSize),
        fontWeight: style.fontWeight,
        fontFamily: style.fontFamily,
        bgAlpha: bg.a,
        contrast: Number(contrastRatio(fg, bg).toFixed(2)),
      });
    }
    return results;
  });
}

test.describe("F8 computed typography & readability gate", () => {
  test("visible_text_minimum_and_contrast across themes, panels, and states", async ({ page }) => {
    await mockGraphApi(page, "populated");
    await page.goto("/");
    await page.evaluate(() => document.fonts.ready);

    // Wait for fixture symbols to load in compatibility or WebGL view.
    await expect(
      page.getByText(/\d+ visible · 10 loaded|architecture groups/i).first(),
    ).toBeVisible();

    for (const theme of ["light", "dark"] as const) {
      await setTheme(page, theme);

      // Compatibility view + Inspector with long path.
      const symbolButton = page.getByRole("button", { name: /compute_blast_radius/ }).first();
      if (await symbolButton.isVisible().catch(() => false)) {
        await symbolButton.click();
      } else {
        await page.keyboard.press("/");
        await page.getByRole("combobox").fill("compute_blast_radius");
        await page.keyboard.press("Enter");
      }
      await expect(page.getByRole("heading", { name: "compute_blast_radius" })).toBeVisible();

      const inspectorAudit = await auditVisibleTypography(page);
      expect(inspectorAudit.checkedCount).toBeGreaterThan(20);
      expect(inspectorAudit.violations, `${theme} inspector`).toEqual([]);

      // Deep Callers tab
      await page.getByRole("tab", { name: "Deep Callers" }).click();
      await expect(page.getByText("HIGH", { exact: true })).toBeVisible();
      const deepAudit = await auditVisibleTypography(page);
      expect(deepAudit.violations, `${theme} deep callers`).toEqual([]);

      // SearchPalette
      await page.getByRole("button", { name: /Find a symbol/i }).click();
      await expect(page.getByRole("combobox")).toBeVisible();
      const searchAudit = await auditVisibleTypography(page);
      expect(searchAudit.violations, `${theme} search palette`).toEqual([]);
      await page.keyboard.press("Escape");

      // KeyboardHelp
      await page.getByRole("button", { name: "Keyboard shortcuts" }).click();
      await expect(page.getByText("Show this help")).toBeVisible();
      const helpAudit = await auditVisibleTypography(page);
      expect(helpAudit.violations, `${theme} keyboard help`).toEqual([]);
      await page.keyboard.press("Escape");
    }

    // Empty state
    await page.unrouteAll();
    await mockGraphApi(page, "empty");
    await page.goto("/");
    await expect(page.getByText("No indexed symbols on this branch")).toBeVisible();
    const emptyAudit = await auditVisibleTypography(page);
    expect(emptyAudit.violations, "empty state").toEqual([]);

    // Error state
    await page.unrouteAll();
    await mockGraphApi(page, "error");
    await page.goto("/");
    await expect(page.getByText(/The graph could not be loaded/i)).toBeVisible();
    const errorAudit = await auditVisibleTypography(page);
    expect(errorAudit.violations, "error state").toEqual([]);

    // Loading state
    await page.unrouteAll();
    await mockGraphApi(page, "loading");
    await page.goto("/");
    await expect(page.getByText(/Reading graph manifest/i)).toBeVisible();
    const loadingAudit = await auditVisibleTypography(page);
    expect(loadingAudit.violations, "loading state").toEqual([]);
  });

  test("labels_remain_legible_across_zoom in WebGL renderer", async ({ page }) => {
    mkdirSync("test-results/diagnostics", { recursive: true });
    await mockGraphApi(page, "populated");
    await page.goto("/");
    await page.evaluate(() => document.fonts.ready);

    await ensureWebGlMode(page);

    // Wait for actual visible DOM .graph-point-label elements (no vacuous zero-label pass).
    await expect
      .poll(async () => (await measureVisibleGraphLabels(page)).length, { timeout: 15_000 })
      .toBeGreaterThan(0);

    const waitForVisibleLabels = async (stage: string): Promise<LabelMeasurement[]> => {
      let latest: LabelMeasurement[] = [];
      await expect
        .poll(
          async () => {
            latest = await measureVisibleGraphLabels(page);
            return latest.length;
          },
          {
            message: `${stage}: expected visible .graph-point-label nodes`,
            timeout: 10_000,
          },
        )
        .toBeGreaterThan(0);
      return latest;
    };

    const assertLabelsReadable = (labels: LabelMeasurement[], stage: string) => {
      expect(labels.length, `${stage}: expected visible .graph-point-label nodes`).toBeGreaterThan(
        0,
      );
      for (const label of labels) {
        expect(label.fontSize, `${stage} (${label.text}) fontSize`).toBeGreaterThanOrEqual(12);
        expect(label.fontFamily, `${stage} (${label.text}) fontFamily`).toContain("IBM Plex Mono");
        expect(label.fontWeight, `${stage} (${label.text}) fontWeight`).toBe("500");
        expect(label.bgAlpha, `${stage} (${label.text}) opaque bg`).toBeGreaterThanOrEqual(0.99);
        expect(label.contrast, `${stage} (${label.text}) contrast`).toBeGreaterThanOrEqual(4.5);
      }
    };

    for (const theme of ["light", "dark"] as const) {
      await setTheme(page, theme);
      await page.getByRole("button", { name: "Fit graph" }).click();
      await page.waitForTimeout(450);
      assertLabelsReadable(
        await waitForVisibleLabels(`${theme} default zoom`),
        `${theme} default zoom`,
      );

      // Zoom in via supported UI control (2x ~1.96x keeps centered clusters in view under software WebGL)
      const zoomIn = page.getByRole("button", { name: "Zoom in" });
      for (let i = 0; i < 2; i += 1) {
        await zoomIn.click();
        await page.waitForTimeout(300);
      }
      assertLabelsReadable(await waitForVisibleLabels(`${theme} zoomed in`), `${theme} zoomed in`);

      // Zoom out via supported UI control (3 clicks from 1.96x => ~0.71x)
      const zoomOut = page.getByRole("button", { name: "Zoom out" });
      for (let i = 0; i < 3; i += 1) {
        await zoomOut.click();
        await page.waitForTimeout(300);
      }
      assertLabelsReadable(
        await waitForVisibleLabels(`${theme} zoomed out`),
        `${theme} zoomed out`,
      );

      await page.getByRole("button", { name: "Fit graph" }).click();
      await page.waitForTimeout(450);

      // Non-blocking diagnostic capture
      await page.screenshot({
        path: `test-results/diagnostics/webgl-${theme}.png`,
        fullPage: false,
      });
    }
  });

  test("fonts_load_without_synthetic_weights", async ({ page }) => {
    await mockGraphApi(page, "populated");
    await page.goto("/");
    await expect(
      page.getByText(/\d+ visible · 10 loaded|architecture groups/i).first(),
    ).toBeVisible();

    const fontAudit = await page.evaluate(async () => {
      await document.fonts.ready;
      const loadedFaces = [...document.fonts]
        .filter((f) => f.status === "loaded")
        .map((f) => `${f.family.replace(/['"]/g, "")}:${f.weight}`);

      const syntheticElements: { text: string; family: string; weight: string }[] = [];
      for (const el of document.querySelectorAll("body *")) {
        if (el.closest("svg, script, style")) continue;
        const style = getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        if (style.display === "none" || rect.width <= 0 || rect.height <= 0) continue;
        const directText = [...el.childNodes]
          .filter((n) => n.nodeType === Node.TEXT_NODE)
          .map((n) => n.textContent ?? "")
          .join("")
          .trim();
        if (!directText) continue;

        const w = Number(style.fontWeight);
        if (![400, 500, 600].includes(w)) {
          syntheticElements.push({
            text: directText.slice(0, 40),
            family: style.fontFamily,
            weight: style.fontWeight,
          });
        }
      }
      return { loadedFaces, syntheticElements };
    });

    expect(fontAudit.loadedFaces).toEqual(
      expect.arrayContaining(["Space Grotesk:400", "Space Grotesk:600"]),
    );
    expect(fontAudit.syntheticElements).toEqual([]);
  });

  test("narrow_and_wide_layouts_avoid_overflow", async ({ page }) => {
    await mockGraphApi(page, "populated");

    for (const viewport of [
      { width: 960, height: 720, name: "narrow" },
      { width: 1440, height: 900, name: "wide" },
    ]) {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      await page.goto("/");
      await page.evaluate(() => document.fonts.ready);
      await expect(
        page.getByText(/\d+ visible · 10 loaded|architecture groups/i).first(),
      ).toBeVisible();

      // Open Inspector on the long-path symbol
      const symbolButton = page.getByRole("button", { name: /compute_blast_radius/ }).first();
      if (await symbolButton.isVisible().catch(() => false)) {
        await symbolButton.click();
      } else {
        await page.keyboard.press("/");
        await page.getByRole("combobox").fill("compute_blast_radius");
        await page.keyboard.press("Enter");
      }
      await expect(page.getByRole("heading", { name: "compute_blast_radius" })).toBeVisible();

      for (const theme of ["light", "dark"] as const) {
        await setTheme(page, theme);
        const overflow = await page.evaluate(() => ({
          scrollWidth: document.documentElement.scrollWidth,
          innerWidth: window.innerWidth,
        }));
        expect(
          overflow.scrollWidth,
          `${viewport.name} ${theme} horizontal overflow`,
        ).toBeLessThanOrEqual(overflow.innerWidth + 1);

        const audit = await auditVisibleTypography(page);
        expect(audit.violations, `${viewport.name} ${theme} typography`).toEqual([]);
      }
    }
  });

  test("compatibility_view_meets_typography_and_contrast", async ({ page }) => {
    mkdirSync("test-results/diagnostics", { recursive: true });
    await page.addInitScript(() => {
      const orig: typeof HTMLCanvasElement.prototype.getContext = Reflect.get(
        HTMLCanvasElement.prototype,
        "getContext",
      );
      HTMLCanvasElement.prototype.getContext = function (
        this: HTMLCanvasElement,
        type: string,
        ...args: unknown[]
      ): RenderingContext | null {
        if (type === "webgl2" || type === "webgl") return null;
        return orig.apply(this, [type, ...args] as Parameters<typeof orig>);
      } as typeof orig;
    });
    await mockGraphApi(page, "populated");
    await page.goto("/");
    await page.evaluate(() => document.fonts.ready);
    await expect(page.getByText("Compatibility atlas")).toBeVisible();

    for (const theme of ["light", "dark"] as const) {
      await setTheme(page, theme);
      const audit = await auditVisibleTypography(page);
      expect(audit.checkedCount).toBeGreaterThan(20);
      expect(audit.violations, `compatibility ${theme}`).toEqual([]);
      await page.screenshot({
        path: `test-results/diagnostics/compatibility-${theme}.png`,
        fullPage: false,
      });
    }
  });
});
