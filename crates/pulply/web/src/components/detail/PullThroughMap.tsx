// "Map" tab of a container pull-through remote or distribution. Pull-through does not follow
// the repository chain of the regular map: a pull-through distribution points to a remote,
// and Pulp creates a distribution and a repository for each image pulled through it (its
// `distributions` field). The map shows that chain, and the commands clients use.
//
// Clients pull `<host>/<path>/<upstream image>`, where `<path>` is the registry path of the
// pull-through distribution: since pulp_container 2.30 it is kept in a label and `base_path`
// is a UUID (see `pullThroughPath`). Pulp refuses anonymous pulls through a pull-through
// cache, public or not: clients log in first.

import {useQuery} from "@tanstack/react-query";
import {ChevronRight, Download, Globe, KeyRound, Layers, Server} from "lucide-react";
import {Fragment, type ReactNode, useId, useState} from "react";

import type {Json} from "../../lib/api";
import {imageName, pullThroughPath, upstreamImage} from "../../lib/container";
import type {GraphNode} from "../../lib/graph";
import type {ResourceType} from "../../lib/registry";
import {useSession} from "../../lib/session";
import {Commands, useOrigin} from "../AddContentHelp";
import {CopyButton} from "../CopyButton";
import {Input} from "../ui/input";
import {Select, SelectContent, SelectItem, SelectTrigger, SelectValue} from "../ui/select";
import {Skeleton} from "../ui/skeleton";
import {NodeCard} from "./RelationMap";

const str = (v: unknown) => (typeof v === "string" && v ? v : null);

/** Cached images listed at most (fetched in one request). */
const MAX_CACHED = 100;
/** Cached images given a ready pull command in the Clients column. */
const MAX_CLIENT_LINES = 8;

interface Column {
  title: string;
  help: string;
  icon: ReactNode;
  empty: string;
  nodes: GraphNode[];
}

/** Ready-to-copy login and pull commands for an image typed by the user. */
function PullCommands({
  dists,
  host,
  insecure,
  upstream,
  example,
}: {
  dists: Json[];
  host: string;
  insecure: boolean;
  upstream: string | null;
  example: string;
}) {
  const [image, setImage] = useState(example);
  const [distHref, setDistHref] = useState(dists[0].pulp_href as string);
  const dist = dists.find((d) => d.pulp_href === distHref) ?? dists[0];
  const id = useId();
  const clean = upstreamImage(image);
  const ref = `${host}/${pullThroughPath(dist) ?? "<path>"}/${clean || "<image>:<tag>"}`;
  const tls = insecure ? "--tls-verify=false " : "";
  return (
    <div className="flex flex-col gap-4 rounded-xl border p-5">
      <div className="flex flex-col gap-1">
        <h3 className="text-sm font-semibold">Pull an image through this cache</h3>
        <p className="text-sm text-muted-foreground">
          Type the image as it is named on the upstream registry{upstream ? <> ({upstream.replace(/^https?:\/\//, "")})</> : null}: the first pull fetches
          and caches it, the next ones are served by Pulp.
        </p>
      </div>
      <div className="flex flex-wrap items-end gap-3">
        <div className="flex min-w-64 flex-1 flex-col gap-1.5">
          <label className="text-sm font-medium" htmlFor={id}>
            Upstream image
          </label>
          <Input className="font-mono" id={id} placeholder="library/alpine:latest" spellCheck={false} value={image} onChange={(e) => setImage(e.target.value)} />
        </div>
        {dists.length > 1 && (
          <div className="flex flex-col gap-1.5">
            <span className="text-sm font-medium">Distribution</span>
            <Select value={distHref} onValueChange={setDistHref}>
              <SelectTrigger className="min-w-48">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {dists.map((d) => (
                  <SelectItem key={d.pulp_href as string} value={d.pulp_href as string}>
                    {str(d.name) ?? pullThroughPath(d)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        )}
      </div>
      <div className="flex flex-col gap-2">
        <span className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          <KeyRound className="size-3.5" /> Docker
        </span>
        <Commands lines={[`docker login ${host}`, `docker pull ${ref}`]} />
        <span className="flex items-center gap-1.5 pt-1 text-xs font-medium text-muted-foreground">
          <KeyRound className="size-3.5" /> Podman
        </span>
        <Commands lines={[`podman login ${tls}${host}`, `podman pull ${tls}${ref}`]} />
      </div>
      <ul className="flex list-disc flex-col gap-1 ps-5 text-xs text-muted-foreground">
        <li>
          Log in with a Pulp user to pull an image not cached yet: Pulp never fills the cache for anonymous clients, even through a public
          distribution. Once cached, an image of a public distribution can be pulled without login (but is then not refreshed from upstream).
        </li>
        {insecure && (
          <li>
            The server is in plain HTTP: Docker needs <span className="font-mono">{`{"insecure-registries": ["${host}"]}`}</span> in{" "}
            <span className="font-mono">/etc/docker/daemon.json</span>, then a restart of the daemon.
          </li>
        )}
        <li>
          Once cached, the image is also a regular distribution: <span className="font-mono">{ref}</span> keeps working if the upstream registry is
          unreachable.
        </li>
      </ul>
    </div>
  );
}

/** A registry reference that wraps after its slashes only (never inside `host:port`). */
function Ref({value}: {value: string}) {
  const parts = value.split("/");
  return (
    <>
      {parts.map((p, i) => (
        <Fragment key={i}>
          {p}
          {i < parts.length - 1 && (
            <>
              /<wbr />
            </>
          )}
        </Fragment>
      ))}
    </>
  );
}

/** One copyable line of the Clients column. */
function ClientLine({title, text, copy, hint}: {title: string; text: ReactNode; copy: string; hint: string}) {
  return (
    <div className="flex items-start gap-1 rounded-lg border bg-background px-3 py-2 text-sm shadow-xs">
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-xs text-muted-foreground">{title}</span>
        <span className="font-mono text-xs leading-5 break-words">{text}</span>
      </span>
      <CopyButton label={hint} text={copy} />
    </div>
  );
}

export function PullThroughMap({href, item, type}: {href: string; item: Json; type: ResourceType}) {
  const {pulp, ctx, registry} = useSession();
  const {origin} = useOrigin();
  const isDist = type.category === "distributions";
  const remoteHref = isDist ? str(item.remote) : href;
  const remote = useQuery({
    queryKey: ["pulp", ctx, remoteHref, null],
    queryFn: () => pulp.get(remoteHref!),
    enabled: !!remoteHref && isDist,
  });
  const remoteItem = isDist ? remote.data : item;

  // From the remote: the pull-through distributions using it (no `remote` filter on the list).
  const distType = registry.types.find((t) => t.category === "distributions" && t.plugin === type.plugin && t.name === "pull-through");
  const dists = useQuery({
    queryKey: ["pulp", ctx, "pull-through-dists", distType?.id],
    queryFn: () => pulp.all(distType!.list_path!, {}, 500),
    enabled: !isDist && !!distType?.list_path,
  });
  const ptDists: Json[] = isDist ? [item] : (dists.data ?? []).filter((d) => d.remote === href);

  const cachedHrefs = [...new Set(ptDists.flatMap((d) => (Array.isArray(d.distributions) ? (d.distributions as string[]) : [])))];
  // One request for all the cached images instead of one per image.
  const cachedPath = registry.get("distributions/container/container")?.list_path;
  const shown = cachedHrefs.slice(0, MAX_CACHED);
  const cached = useQuery({
    queryKey: ["pulp", ctx, "pull-through-cached", shown],
    queryFn: async () =>
      (
        await pulp.list(cachedPath!, {
          pulp_href__in: shown.join(","),
          limit: MAX_CACHED,
          ordering: "base_path",
          fields: "pulp_href,name,base_path,private",
        })
      ).results,
    enabled: !!cachedPath && shown.length > 0,
  });

  const loading = (isDist && remote.isLoading) || (!isDist && dists.isLoading) || cached.isLoading;
  if (loading) return <Skeleton className="h-64" />;

  const host = origin ? new URL(origin).host : "<registry>";
  const prefixes = ptDists.map(pullThroughPath).filter((p): p is string => !!p);
  // Cached image names, relative to the pull-through path they were pulled under.
  const images = (cached.data ?? []).map((d) => {
    const path = str(d.base_path) ?? "";
    const prefix = prefixes.find((p) => path.startsWith(`${p}/`)) ?? null;
    // Not `registry_path`: Pulp builds it from the host the API was called with (pulply may
    // reach Pulp as `localhost`), clients use the content origin.
    return {dist: d, name: imageName(path, prefix), ref: `${host}/${path}`};
  });
  const columns: Column[] = [
    {
      title: "Pull-through remote",
      help: "The upstream registry images are fetched from",
      icon: <Globe />,
      empty: "No remote",
      nodes: remoteItem
        ? [{id: remoteItem.pulp_href as string, kind: "remote", label: str(remoteItem.name) ?? "Remote", detail: str(remoteItem.url) ?? undefined}]
        : [],
    },
    {
      title: "Pull-through distribution",
      help: "The path clients pull through",
      icon: <Server />,
      empty: "No pull-through distribution: create one from the remote (New pull-through distribution)",
      nodes: ptDists.map((d) => ({
        id: d.pulp_href as string,
        kind: "distribution",
        label: str(d.name) ?? "Distribution",
        detail: `${host}/${pullThroughPath(d) ?? "<path>"}/`,
      })),
    },
    {
      title: "Cached images",
      help: "Created by Pulp for each image pulled, with its repository",
      icon: <Layers />,
      empty: "Nothing cached yet: images appear here once pulled through the distribution",
      nodes: images.map(({dist: d, name}) => ({
        id: d.pulp_href as string,
        kind: "distribution",
        label: name,
        detail: `${d.private === true ? "private · " : ""}${str(d.base_path)}`,
      })),
    },
    {title: "Clients", help: "What docker or podman pull, after a login", icon: <Download />, empty: "No path yet", nodes: []},
  ];

  return (
    <div className="flex flex-col gap-3">
      <p className="text-sm text-muted-foreground">
        Pull-through works without syncing: a client pulls through the distribution, Pulp fetches the image from the remote, and keeps it
        in a repository and a distribution it creates for that image. Click any box to open it.
      </p>
      <div className="overflow-x-auto rounded-xl border bg-muted/30">
        <div
          className="grid gap-x-2 gap-y-3 p-5"
          style={{gridTemplateColumns: columns.map(() => "minmax(170px, 1fr)").join(" 24px "), minWidth: columns.length * 220}}
        >
          {columns.map((c, i) => (
            <Fragment key={c.title}>
              {i > 0 && <span />}
              <div className="flex flex-col gap-1 pb-2">
                <span className="flex items-center gap-1.5 text-sm font-semibold [&_svg]:size-4 [&_svg]:text-muted-foreground">
                  {c.icon}
                  {c.title}
                </span>
                <span className="text-xs leading-snug text-muted-foreground">{c.help}</span>
              </div>
            </Fragment>
          ))}
          {columns.map((c, i) => (
            <Fragment key={c.title}>
              {i > 0 && (
                <span aria-hidden className="flex items-center justify-center text-muted-foreground/60" style={{gridRow: 2}}>
                  <ChevronRight className="size-5" />
                </span>
              )}
              <div className="flex flex-col justify-center gap-3" style={{gridRow: 2}}>
                {c.nodes.map((n) => (
                  <NodeCard key={n.id} dimmed={false} focus={n.id === href} node={n} />
                ))}
                {c.title === "Clients" &&
                  ptDists.map((d) => {
                    const prefix = `${host}/${pullThroughPath(d) ?? "<path>"}/`;
                    return (
                      <ClientLine
                        key={d.pulp_href as string}
                        copy={prefix}
                        hint="Copy the registry prefix"
                        text={
                          <>
                            <Ref value={prefix} />
                            <span className="text-muted-foreground">{"<image>:<tag>"}</span>
                          </>
                        }
                        title="Any image of the remote"
                      />
                    );
                  })}
                {c.title === "Clients" &&
                  images.slice(0, MAX_CLIENT_LINES).map(({dist, ref}) => (
                    <ClientLine
                      key={dist.pulp_href as string}
                      copy={`docker pull ${ref}`}
                      hint="Copy the pull command"
                      text={<Ref value={ref} />}
                      title="docker pull"
                    />
                  ))}
                {c.title === "Clients" && images.length > MAX_CLIENT_LINES && (
                  <span className="px-1 text-xs text-muted-foreground">and {images.length - MAX_CLIENT_LINES} more cached images</span>
                )}
                {!c.nodes.length && !(c.title === "Clients" && ptDists.length) && (
                  <span className="rounded-lg border border-dashed bg-background/60 px-3 py-2 text-xs text-muted-foreground">{c.empty}</span>
                )}
              </div>
            </Fragment>
          ))}
        </div>
      </div>
      {ptDists.length > 0 && (
        <PullCommands
          dists={ptDists}
          example={images[0] ? `${images[0].name}:latest` : "library/alpine:latest"}
          host={host}
          insecure={!!origin && origin.startsWith("http:")}
          upstream={str(remoteItem?.url)}
        />
      )}
      {cachedHrefs.length > MAX_CACHED && (
        <p className="text-sm text-muted-foreground">
          {MAX_CACHED} of {cachedHrefs.length} cached images shown.
        </p>
      )}
    </div>
  );
}
