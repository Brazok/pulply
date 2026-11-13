// "Map" tab: the chain a resource belongs to, from the remote content is synced from to the
// URL clients download from, with the current resource highlighted.

import {useQueries, useQuery} from "@tanstack/react-query";
import {BookOpen, Database, Download, Globe, Layers, Server} from "lucide-react";
import {type ReactNode, useLayoutEffect, useMemo, useRef, useState} from "react";
import {Link} from "react-router";

import type {Json} from "../../lib/api";
import {type Graph, type GraphNode, type NodeKind, KINDS, buildGraph, connected} from "../../lib/graph";
import {type ResourceType, versionRepository} from "../../lib/registry";
import {itemUrl} from "../../lib/routes";
import {originHost} from "../../lib/container";
import {useOrigin} from "../AddContentHelp";
import {useSession} from "../../lib/session";
import {cn} from "../../lib/utils";
import {Badge} from "../ui/badge";
import {Skeleton} from "../ui/skeleton";

const COLUMNS: Record<NodeKind, {title: string; help: string; icon: ReactNode; empty: string}> = {
  remote: {title: "Remote", help: "Where the content is synced from", icon: <Globe />, empty: "No remote: content is uploaded"},
  repository: {title: "Repository", help: "Holds the content; every change makes a new version", icon: <Database />, empty: "No repository"},
  version: {title: "Versions", help: "Snapshots of the repository's content", icon: <Layers />, empty: "No version yet"},
  publication: {
    title: "Publications",
    help: "A version prepared for clients (metadata)",
    icon: <BookOpen />,
    empty: "Not published: a distribution can serve the repository directly",
  },
  distribution: {title: "Distributions", help: "Serve content at a URL", icon: <Server />, empty: "Not distributed: clients cannot reach it yet"},
  url: {title: "Clients", help: "Where dnf, pip, podman… download from", icon: <Download />, empty: "No URL yet"},
};

/** Types with a map: the steps of the chain. */
export function hasRelationMap(type: ResourceType | undefined, isVersion: boolean): boolean {
  return isVersion || (!!type && !type.parent && ["repositories", "remotes", "publications", "distributions"].includes(type.category));
}

const str = (v: unknown) => (typeof v === "string" && v ? v : null);

/** Repositories the map is centred on. */
function useAnchor(href: string, item: Json, type: ResourceType | undefined, isVersion: boolean) {
  const {pulp, ctx, registry} = useSession();
  const category = isVersion ? "version" : type?.category;
  const viaPublication = category === "distributions" && !str(item.repository) && !str(item.repository_version) ? str(item.publication) : null;
  const publication = useQuery({
    queryKey: ["pulp", ctx, "map-publication", viaPublication],
    queryFn: () => pulp.get(viaPublication!, {fields: "repository,repository_version"}),
    enabled: !!viaPublication,
  });
  const repositories = registry.get("repositories");
  const users = useQuery({
    queryKey: ["pulp", ctx, "map-remote-users", href],
    queryFn: () => pulp.list(repositories!.list_path!, {remote: href, limit: 50}),
    enabled: category === "remotes" && !!repositories?.list_path,
  });
  if (category === "remotes") return {repoHrefs: users.data?.results.map((r) => r.pulp_href as string), loading: users.isLoading};
  let repo: string | null = null;
  if (category === "repositories") repo = href;
  else if (category === "version") repo = versionRepository(href);
  else if (category === "publications") repo = str(item.repository) ?? versionRepository(str(item.repository_version) ?? "");
  else if (category === "distributions") {
    const source = viaPublication ? publication.data : item;
    repo = str(source?.repository) ?? versionRepository(str(source?.repository_version) ?? "");
  }
  return {repoHrefs: repo ? [repo] : viaPublication && publication.isLoading ? undefined : [], loading: publication.isLoading};
}

function useGraph(repoHrefs: string[] | undefined): {graph: Graph | null; partial: boolean} {
  const {pulp, ctx, registry} = useSession();
  const registryHost = originHost(useOrigin().origin);
  const single = repoHrefs?.length === 1 ? repoHrefs[0] : null;
  const repos = useQueries({
    queries: (repoHrefs ?? []).map((h) => ({queryKey: ["pulp", ctx, h, null], queryFn: () => pulp.get(h)})),
  });
  const repoData = repos.map((q) => q.data).filter((d): d is Json => !!d);
  const remoteHrefs = [...new Set(repoData.map((r) => str(r.remote)).filter((h): h is string => !!h))];
  const remotes = useQueries({
    queries: remoteHrefs.map((h) => ({queryKey: ["pulp", ctx, h, null], queryFn: () => pulp.get(h)})),
  });
  const versions = useQuery({
    queryKey: ["pulp", ctx, "map-versions", single],
    queryFn: () => pulp.list(`${single}versions/`, {limit: 3, fields: "pulp_href,number,content_summary"}),
    enabled: !!single,
  });
  const publicationsType = registry.get("publications");
  const publications = useQuery({
    queryKey: ["pulp", ctx, "map-publications", single],
    queryFn: () => pulp.list(publicationsType!.list_path!, {repository: single!, limit: 50}),
    enabled: !!single && !!publicationsType?.list_path,
  });
  // Distributions cannot be filtered by publication: the plugin's are filtered here.
  const plugin = single ? registry.typeForHref(single)?.plugin : undefined;
  const distTypes = registry.topLevel().filter((t) => t.category === "distributions" && t.plugin === plugin && t.list_path);
  const distributions = useQueries({
    queries: distTypes.map((t) => ({
      queryKey: ["pulp", ctx, "map-distributions", t.id],
      queryFn: () => pulp.list(t.list_path!, {limit: 500}),
      enabled: !!single,
    })),
  });

  const loading = repos.some((q) => q.isLoading) || versions.isLoading || publications.isLoading || distributions.some((q) => q.isLoading);
  return useMemo(() => {
    if (!repoHrefs || loading) return {graph: null, partial: false};
    const pubs = publications.data?.results ?? [];
    const pubSet = new Set(pubs.map((p) => p.pulp_href as string));
    const repoSet = new Set(repoHrefs);
    const dists = distributions
      .flatMap((q) => q.data?.results ?? [])
      .filter(
        (d) =>
          repoSet.has(str(d.repository) ?? "") ||
          pubSet.has(str(d.publication) ?? "") ||
          repoSet.has(versionRepository(str(d.repository_version) ?? "") ?? ""),
      );
    const graph = buildGraph({
      repositories: repoData,
      remotes: remotes.map((q) => q.data).filter((d): d is Json => !!d),
      versions: versions.data?.results ?? [],
      publications: pubs,
      distributions: dists,
      registryHost,
    });
    return {graph, partial: !single && repoHrefs.length > 0};
    // Recomputed when a query gets new data: keeps the graph (and the measured lines) stable.
  }, [loading, registryHost, repoHrefs?.join(), repos.map((q) => q.dataUpdatedAt).join(), remotes.map((q) => q.dataUpdatedAt).join(), versions.dataUpdatedAt, publications.dataUpdatedAt, distributions.map((q) => q.dataUpdatedAt).join()]);
}

interface Line {
  d: string;
  on: boolean;
  /** Edge skipping columns: drawn dashed in a lane under the map, with its label. */
  label?: {text: string; x: number; y: number};
}

/** Height of one lane under the map. */
const LANE = 30;

/** Edges jumping over at least one column (a distribution serving a repository directly). */
function skipping(graph: Graph, kinds: NodeKind[]) {
  const column = new Map(graph.nodes.map((n) => [n.id, kinds.indexOf(n.kind)]));
  return graph.edges.filter((e) => (column.get(e.to) ?? 0) - (column.get(e.from) ?? 0) > 1);
}

/** Lines between the node cards, measured once laid out. */
function useLines(graph: Graph | null, kinds: NodeKind[], container: React.RefObject<HTMLDivElement | null>, path: Set<string>): Line[] {
  const [lines, setLines] = useState<Line[]>([]);
  useLayoutEffect(() => {
    const box = container.current;
    if (!box || !graph) return;
    const skip = skipping(graph, kinds);
    const measure = () => {
      const origin = box.getBoundingClientRect();
      const rect = (id: string) => box.querySelector(`[data-node="${CSS.escape(id)}"]`)?.getBoundingClientRect();
      const bottom = Math.max(...[...box.querySelectorAll("[data-node]")].map((n) => n.getBoundingClientRect().bottom)) - origin.top;
      setLines(
        graph.edges.flatMap((e) => {
          const a = rect(e.from);
          const b = rect(e.to);
          if (!a || !b) return [];
          const on = path.has(e.from) && path.has(e.to);
          const lane = skip.indexOf(e);
          if (lane >= 0) {
            // Down from the source, along a lane under every card, up into the target.
            const y = bottom + 18 + lane * LANE;
            const xa = a.left + a.width / 2 - origin.left;
            const xb = b.left + b.width / 2 - origin.left;
            const ya = a.bottom - origin.top;
            const yb = b.bottom - origin.top + 6;
            const d = `M ${xa} ${ya} L ${xa} ${y - 8} Q ${xa} ${y} ${xa + 8} ${y} L ${xb - 8} ${y} Q ${xb} ${y} ${xb} ${y - 8} L ${xb} ${yb}`;
            return [{d, on, label: e.label ? {text: e.label, x: (xa + xb) / 2, y: y + 14} : undefined}];
          }
          const x1 = a.right - origin.left;
          const y1 = a.top + a.height / 2 - origin.top;
          const x2 = b.left - origin.left - 6;
          const y2 = b.top + b.height / 2 - origin.top;
          const dx = Math.max(24, (x2 - x1) / 2);
          return [{d: `M ${x1} ${y1} C ${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}`, on}];
        }),
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(box);
    return () => observer.disconnect();
  }, [graph, kinds, container, path]);
  return lines;
}

export function NodeCard({node, focus, dimmed}: {node: GraphNode; focus: boolean; dimmed: boolean}) {
  const {ctx} = useSession();
  const body = (
    <>
      {focus && <span className="text-[11px] font-semibold tracking-wide text-primary uppercase">You are here</span>}
      <span className="flex items-center gap-1.5 font-medium">
        <span className="truncate">{node.label}</span>
        {node.latest && <Badge variant="info">latest</Badge>}
      </span>
      {node.detail && <span className="truncate text-xs text-muted-foreground">{node.detail}</span>}
    </>
  );
  const className = cn(
    "relative z-10 flex flex-col gap-0.5 rounded-lg border bg-background px-3 py-2 text-left text-sm shadow-xs transition-colors hover:border-primary/60",
    focus && "border-primary bg-primary/5 ring-2 ring-primary/25",
    dimmed && "opacity-50",
  );
  if (node.kind === "url" && !/^https?:/.test(node.id)) {
    return (
      <div className={className} data-node={node.id} title={node.id}>
        {body}
      </div>
    );
  }
  if (node.kind === "url") {
    return (
      <a className={className} data-node={node.id} href={node.id} rel="noreferrer" target="_blank" title={node.id}>
        {body}
      </a>
    );
  }
  return (
    <Link className={className} data-node={node.id} title={node.id} to={itemUrl(ctx, node.id)}>
      {body}
    </Link>
  );
}

export function RelationMap({href, item, type, isVersion}: {href: string; item: Json; type: ResourceType | undefined; isVersion: boolean}) {
  const {registry} = useSession();
  const anchor = useAnchor(href, item, type, isVersion);
  const {graph, partial} = useGraph(anchor.repoHrefs);
  const container = useRef<HTMLDivElement>(null);
  const path = useMemo(() => (graph ? connected(graph, href) : new Set<string>()), [graph, href]);
  const plugin = type?.plugin ?? "core";
  const hasPublications = !!registry.pluginType("publications", plugin, false);
  const kinds = useMemo(
    () =>
      KINDS.filter((k) => {
        if (partial) return k === "remote" || k === "repository";
        // Plugins without publications (container, python…) serve repositories directly.
        if (k === "publication") return hasPublications || !!graph?.nodes.some((n) => n.kind === k);
        return true;
      }),
    [graph, partial, hasPublications],
  );
  const lanes = graph ? skipping(graph, kinds).length : 0;
  const lines = useLines(graph, kinds, container, path);

  if (!graph) {
    return anchor.repoHrefs?.length === 0 && !anchor.loading ? (
      <p className="text-sm text-muted-foreground">This resource is not linked to a repository yet.</p>
    ) : (
      <Skeleton className="h-64" />
    );
  }
  const byKind = (k: NodeKind) => graph.nodes.filter((n) => n.kind === k);
  // Only what the resource touches is emphasised; the repository shows its whole chain.
  const dim = (id: string) => !path.has(id);

  return (
    <div className="flex flex-col gap-3">
      <p className="text-sm text-muted-foreground">
        Content moves left to right: a remote is <b className="font-medium text-foreground">synced</b> into a repository, each change
        makes a new <b className="font-medium text-foreground">version</b>, a version is <b className="font-medium text-foreground">published</b>
        , and a distribution <b className="font-medium text-foreground">serves</b> it at a URL. Click any box to open it.
      </p>
      <div className="overflow-x-auto rounded-xl border bg-muted/30">
        <div
          ref={container}
          className="relative grid gap-x-12 gap-y-3 p-5"
          style={{
            gridTemplateColumns: `repeat(${kinds.length}, minmax(170px, 1fr))`,
            minWidth: kinds.length * 210,
            paddingBottom: lanes ? 32 + lanes * LANE : undefined,
          }}
        >
          <svg aria-hidden className="pointer-events-none absolute inset-0 size-full overflow-visible">
            <defs>
              {["on", "off"].map((s) => (
                <marker key={s} id={`map-arrow-${s}`} markerHeight="8" markerWidth="8" orient="auto" refX="6" refY="4">
                  <path className={s === "on" ? "fill-primary" : "fill-muted-foreground/40"} d="M0,0 L8,4 L0,8 z" />
                </marker>
              ))}
            </defs>
            {lines.map((l, i) => (
              <g key={i}>
                <path
                  className={l.on ? "stroke-primary" : "stroke-muted-foreground/30"}
                  d={l.d}
                  fill="none"
                  markerEnd={`url(#map-arrow-${l.on ? "on" : "off"})`}
                  strokeDasharray={l.label ? "5 4" : undefined}
                  strokeWidth={l.on ? 2 : 1.5}
                />
                {l.label && (
                  <text
                    className={cn("text-[11px] font-medium", l.on ? "fill-primary" : "fill-muted-foreground")}
                    textAnchor="middle"
                    x={l.label.x}
                    y={l.label.y}
                  >
                    {l.label.text}
                  </text>
                )}
              </g>
            ))}
          </svg>
          {kinds.map((k) => (
            <div key={k} className="flex flex-col gap-1 pb-2">
              <span className="flex items-center gap-1.5 text-sm font-semibold [&_svg]:size-4 [&_svg]:text-muted-foreground">
                {COLUMNS[k].icon}
                {COLUMNS[k].title}
              </span>
              <span className="text-xs leading-snug text-muted-foreground">{COLUMNS[k].help}</span>
            </div>
          ))}
          {kinds.map((k) => (
            <div key={k} className="flex flex-col justify-center gap-3" style={{gridRow: 2}}>
              {byKind(k).map((n) => (
                <NodeCard key={n.id} dimmed={dim(n.id)} focus={n.id === href} node={n} />
              ))}
              {!byKind(k).length && (
                <span className="relative z-10 rounded-lg border border-dashed bg-background/60 px-3 py-2 text-xs text-muted-foreground">
                  {COLUMNS[k].empty}
                </span>
              )}
            </div>
          ))}
        </div>
      </div>
      {partial && (
        <p className="text-sm text-muted-foreground">Open one of these repositories to see its versions, publications and distributions.</p>
      )}
    </div>
  );
}
