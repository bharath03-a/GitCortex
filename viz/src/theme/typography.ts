/** Read the shared CSS pixel size; reject unresolved/non-pixel tokens. */
export function readGraphLabelSize(root: Element): number {
  const token = getComputedStyle(root).getPropertyValue("--text-graph-label").trim();
  const size = /^(?:\d+\.?\d*|\.\d+)px$/.test(token) ? Number.parseFloat(token) : NaN;
  return Number.isFinite(size) ? Math.max(12, size) : 13;
}
