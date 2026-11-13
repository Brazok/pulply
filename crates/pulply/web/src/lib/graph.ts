// Relation map of a repository: where its content comes from (remote), the versions it went
// through, what is published and distributed from them, and where clients download it.
// Pure functions over Pulp objects; the fetching is done by components/detail/RelationMap.

import type {Json} from "./api";
import {registryRef} from "./container";
import {versionNumber, versionRepository} from "./registry";

export type NodeKind = "remote" | "repository" | "version" | "publication" | "distribution" | "url";

/** Columns of the map, left to right: the path content follows. */
export const KINDS: NodeKind[] = ["remote", "repository", "version", "publication", "distribution", "url"];

export interface GraphNode {
  /** Pulp href, or the URL for `url` nodes. */
  id: string;
  kind: NodeKind;
  label: string;
  /** Second line: content count, base path, guard… */
  detail?: string;
  /** Latest version of its repository. */
  latest?: boolean;
}

export interface GraphEdge {
  from: string;
  to: string;
  /** Said along the edge when the link is not obvious (a distribution serving a repository). */
  label?: string;
}

export interface Graph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface GraphInput {
  /** Repositories at the centre of the map (one, or those using a remote). */
  repositories: Json[];
  remotes: Json[];
  /** Recent versions of the repository (others referenced below are added). */
  versions: Json[];
  publications: Json[];
  distributions: Json[];
  /** Host clients reach the registry at (container distributions), from the content origin. */
  registryHost?: string;
}

const str = (v: unknown) => (typeof v === "string" && v ? v : null);

function present(version: Json): number | null {
  const summary = (version.content_summary as {present?: Record<string, {count?: number}>} | undefined)?.present;
  if (!summary) return null;
  return Object.values(summary).reduce((n, t) => n + (t?.count ?? 0), 0);
}

function units(n: number): string {
  return `${n.toLocaleString()} content unit${n === 1 ? "" : "s"}`;
}

export function buildGraph(input: GraphInput): Graph {
  const nodes = new Map<string, GraphNode>();
  const edges: GraphEdge[] = [];
  const add = (node: GraphNode) => {
    if (!nodes.has(node.id)) nodes.set(node.id, node);
  };
  const link = (from: string | null, to: string | null, label?: string) => {
    if (from && to && nodes.has(from) && nodes.has(to) && !edges.some((e) => e.from === from && e.to === to)) {
      edges.push(label ? {from, to, label} : {from, to});
    }
  };
  const addVersion = (href: string, data?: Json) => {
    const n = data ? present(data) : null;
    add({id: href, kind: "version", label: `Version ${versionNumber(href) ?? "?"}`, detail: n === null ? undefined : units(n)});
  };

  for (const r of input.remotes) {
    add({id: r.pulp_href as string, kind: "remote", label: str(r.name) ?? "Remote", detail: str(r.url) ?? undefined});
  }
  for (const repo of input.repositories) {
    const href = repo.pulp_href as string;
    add({id: href, kind: "repository", label: str(repo.name) ?? "Repository", detail: str(repo.description) ?? undefined});
    link(str(repo.remote), href);
  }
  const latest = new Set(input.repositories.map((r) => str(r.latest_version_href)).filter((h): h is string => !!h));
  for (const v of input.versions) addVersion(v.pulp_href as string, v);
  // Versions published or distributed, even when older than the recent ones.
  for (const h of [...input.publications.map((p) => str(p.repository_version)), ...input.distributions.map((d) => str(d.repository_version))]) {
    if (h) addVersion(h);
  }
  for (const node of nodes.values()) {
    if (node.kind === "version") {
      node.latest = latest.has(node.id);
      link(versionRepository(node.id), node.id);
    }
  }
  for (const p of input.publications) {
    const href = p.pulp_href as string;
    const v = str(p.repository_version);
    add({id: href, kind: "publication", label: "Publication", detail: v ? `of version ${versionNumber(v)}` : undefined});
    link(v, href);
  }
  for (const d of input.distributions) {
    const href = d.pulp_href as string;
    const guard = str(d.content_guard);
    add({
      id: href,
      kind: "distribution",
      label: str(d.name) ?? "Distribution",
      detail: [str(d.base_path) && `/${d.base_path}`, guard && "protected"].filter(Boolean).join(" · ") || undefined,
    });
    // A distribution serves a publication, a fixed version, or the latest version of a repository.
    link(str(d.publication), href);
    link(str(d.repository_version), href, "serves this version");
    link(str(d.repository), href, "serves the latest version");
    // Container distributions are pulled from a registry path rather than a URL.
    const url = str(d.base_url) ?? (str(d.registry_path) ? registryRef(d, input.registryHost) : null);
    if (url) {
      add({id: url, kind: "url", label: url.replace(/^https?:\/\//, ""), detail: str(d.base_url) ? "clients download from here" : "podman pull from here"});
      link(href, url);
    }
  }
  return {nodes: [...nodes.values()], edges};
}

/** Nodes and edges on a path through `focus` (to highlight what the current resource touches). */
export function connected(graph: Graph, focus: string): Set<string> {
  const keep = new Set([focus]);
  const walk = (dir: "from" | "to") => {
    const other = dir === "from" ? "to" : "from";
    const queue = [focus];
    while (queue.length) {
      const id = queue.shift()!;
      for (const e of graph.edges) {
        if (e[dir] === id && !keep.has(e[other])) {
          keep.add(e[other]);
          queue.push(e[other]);
        }
      }
    }
  };
  walk("from");
  walk("to");
  return keep;
}
