export const KIND_COLOR: Record<string, string> = {
  folder: "#45475a",
  file: "#6c7086",
  module: "#cba6f7",
  struct: "#a6e3a1",
  enum: "#94e2d5",
  trait: "#fab387",
  interface: "#89dceb",
  typealias: "#f38ba8",
  function: "#89b4fa",
  method: "#74c7ec",
  constant: "#f9e2af",
  macro: "#cdd6f4",
  property: "#b4befe",
  annotation: "#eba0ac",
  enummember: "#a6d189",
  section: "#f5c2e7",
};

export const EDGE_COLOR: Record<string, string> = {
  calls: "#89b4fa",
  implements: "#fab387",
  inherits: "#fab387",
  uses: "#74c7ec",
  imports: "#6c7086",
  contains: "#45475a",
  throws: "#f38ba8",
  annotated: "#eba0ac",
  references: "#f5c2e7",
};

export const EDGE_WIDTH: Record<string, number> = {
  calls: 1.6,
  implements: 1.4,
  inherits: 1.4,
  uses: 1.0,
  imports: 0.6,
  contains: 0.4,
  throws: 1.2,
  annotated: 0.8,
  references: 0.8,
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
  extracted: "#a6e3a1",
  resolved: "#f9e2af",
  inferred: "#6c7086",
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
      return 0.32;
    case "resolved":
      return 0.72;
    default:
      return 1.0;
  }
}

export function dimColor(hex: string, amount = 0.7): string {
  const c = hex.replace("#", "");
  const r = parseInt(c.slice(0, 2), 16);
  const g = parseInt(c.slice(2, 4), 16);
  const b = parseInt(c.slice(4, 6), 16);
  const bg = 0x12;
  const mix = (v: number) => Math.round(v * (1 - amount) + bg * amount);
  return `rgb(${mix(r)}, ${mix(g)}, ${mix(b)})`;
}
