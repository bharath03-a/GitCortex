import type { GraphData, RawNode } from "../api";
import { graphClusterForFile } from "./semantics";

export interface GroupSummary {
  name: string;
  nodes: RawNode[];
  relations: number;
  boundaryRelations: number;
  loc: number;
  kinds: Map<string, number>;
}

export function summarizeGroups(data: GraphData): GroupSummary[] {
  const clusterById = new Map<string, string>();
  const groups = new Map<string, GroupSummary>();
  for (const node of data.nodes) {
    const name = graphClusterForFile(node.file);
    clusterById.set(node.id, name);
    const group = groups.get(name) ?? {
      name,
      nodes: [],
      relations: 0,
      boundaryRelations: 0,
      loc: 0,
      kinds: new Map<string, number>(),
    };
    group.nodes.push(node);
    group.loc += node.loc;
    group.kinds.set(node.kind, (group.kinds.get(node.kind) ?? 0) + 1);
    groups.set(name, group);
  }
  for (const edge of data.edges) {
    const source = clusterById.get(edge.src);
    const target = clusterById.get(edge.dst);
    if (source) {
      const group = groups.get(source);
      if (group) {
        group.relations += 1;
        if (target && source !== target) group.boundaryRelations += 1;
      }
    }
    if (target && target !== source) {
      const group = groups.get(target);
      if (group) {
        group.relations += 1;
        group.boundaryRelations += 1;
      }
    }
  }
  for (const group of groups.values()) {
    group.nodes.sort((a, b) => b.loc - a.loc || a.name.localeCompare(b.name));
  }
  return [...groups.values()].sort(
    (a, b) => b.nodes.length - a.nodes.length || a.name.localeCompare(b.name),
  );
}
