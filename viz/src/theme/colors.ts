/* Functional colors mirror the GitCortex product datasheet: one hue per
   semantic family, with neutral structural nodes. */
export type ProductTheme = "light" | "dark";
export const KIND_COLOR: Record<string, string> = {
  folder: "#9A978F",
  file: "#74747D",
  module: "#8A6D28",
  struct: "#2F6F5E",
  enum: "#3C7E6C",
  trait: "#6B4CA8",
  interface: "#7658B4",
  typealias: "#4D7A68",
  function: "#2B5CA8",
  method: "#3970BD",
  constant: "#9A7723",
  macro: "#B05B3D",
  property: "#557DAF",
  annotation: "#A03D6B",
  enummember: "#5E8466",
  section: "#A03D6B",
};

export const DARK_KIND_COLOR: Record<string, string> = {
  folder: "#AAA79F",
  file: "#8E929C",
  module: "#D2AC5C",
  struct: "#72B69F",
  enum: "#64C0A2",
  trait: "#A98BDD",
  interface: "#B79AEB",
  typealias: "#7DB79F",
  function: "#79A7F2",
  method: "#65B0F4",
  constant: "#D7B85F",
  macro: "#E58A67",
  property: "#83AFE0",
  annotation: "#E180AE",
  enummember: "#91BE98",
  section: "#E180AE",
};

export const EDGE_COLOR: Record<string, string> = {
  calls: "#2B5CA8",
  implements: "#6B4CA8",
  inherits: "#7658B4",
  uses: "#2F6F5E",
  imports: "#8A6D28",
  contains: "#AAA79E",
  throws: "#B84B42",
  annotated: "#A03D6B",
  references: "#557DAF",
};

export const DARK_EDGE_COLOR: Record<string, string> = {
  calls: "#79A7F2",
  implements: "#A98BDD",
  inherits: "#B79AEB",
  uses: "#72B69F",
  imports: "#D2AC5C",
  contains: "#777B84",
  throws: "#DF756C",
  annotated: "#E180AE",
  references: "#83AFE0",
};

export function kindColors(theme: ProductTheme): Record<string, string> {
  return theme === "dark" ? DARK_KIND_COLOR : KIND_COLOR;
}

export function edgeColors(theme: ProductTheme): Record<string, string> {
  return theme === "dark" ? DARK_EDGE_COLOR : EDGE_COLOR;
}

export const EDGE_WIDTH: Record<string, number> = {
  calls: 0.72,
  implements: 0.68,
  inherits: 0.68,
  uses: 0.54,
  imports: 0.42,
  contains: 0.32,
  throws: 0.7,
  annotated: 0.48,
  references: 0.48,
};

export const KIND_LABEL: Record<string, string> = {
  folder: "Folder",
  file: "File",
  module: "Module",
  struct: "Struct",
  enum: "Enum",
  trait: "Trait",
  interface: "Interface",
  typealias: "Type Alias",
  function: "Function",
  method: "Method",
  constant: "Constant",
  macro: "Macro",
  property: "Property",
  annotation: "Annotation",
  enummember: "Enum Member",
  section: "Section",
};

export const CONFIDENCE_COLOR: Record<string, string> = {
  extracted: "#2F6F5E",
  resolved: "#8A6D28",
  inferred: "#9A978F",
};

export const CONFIDENCE_LABEL: Record<string, string> = {
  extracted: "Extracted",
  resolved: "Resolved",
  inferred: "Inferred",
};

/** Maps edge confidence tier to an opacity multiplier (0–1). */
export function confidenceAlpha(confidence: string | undefined): number {
  switch (confidence) {
    case "inferred":
      return 0.28;
    case "resolved":
      return 0.62;
    default:
      return 0.86;
  }
}

/** Mix a graph color into the product paper background. */
export function dimColor(hex: string, amount = 0.7, theme: ProductTheme = "light"): string {
  const c = hex.replace("#", "");
  const r = parseInt(c.slice(0, 2), 16);
  const g = parseInt(c.slice(2, 4), 16);
  const b = parseInt(c.slice(4, 6), 16);
  const bg = theme === "dark" ? { r: 0x10, g: 0x12, b: 0x16 } : { r: 0xfc, g: 0xfc, b: 0xfa };
  const mix = (value: number, paper: number) => Math.round(value * (1 - amount) + paper * amount);
  return `rgb(${mix(r, bg.r)}, ${mix(g, bg.g)}, ${mix(b, bg.b)})`;
}
