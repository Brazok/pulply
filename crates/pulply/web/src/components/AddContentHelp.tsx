// "How to add content" for plugins whose content cannot be uploaded from the browser
// (container images are pushed with a registry client, Hugging Face files come through a
// pull-through cache): the ways to do it, with ready-to-copy commands for this server.

import {useQuery} from "@tanstack/react-query";
import {Lightbulb} from "lucide-react";
import type {ReactNode} from "react";
import {Link} from "react-router";

import type {Json} from "../lib/api";
import {pluginLabel} from "../lib/format";
import type {Registry} from "../lib/registry";
import {listUrl} from "../lib/routes";
import {useSession} from "../lib/session";
import {CopyButton} from "./CopyButton";
import {Badge} from "./ui/badge";
import {Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle} from "./ui/dialog";
import {Skeleton} from "./ui/skeleton";

const HELP_PLUGINS = ["container", "hugging_face"];

/** Plugins of the server with a help, whose content cannot be uploaded from the browser. */
export function helpPlugins(reg: Registry): string[] {
  return HELP_PLUGINS.filter((p) => reg.plugins().includes(p) && reg.uploadTypes(p).length === 0);
}

export const hasAddContentHelp = (reg: Registry, plugin: string) => helpPlugins(reg).includes(plugin);

interface Way {
  title: string;
  recommended?: boolean;
  text: ReactNode;
  commands?: string[];
}

export function Commands({lines}: {lines: string[]}) {
  return (
    <ul className="flex flex-col gap-2">
      {lines.map((line) => (
        <li key={line} className="flex items-start gap-2 rounded-lg bg-muted/60 px-3 py-2">
          <pre className="flex-1 overflow-x-auto font-mono text-xs leading-5 whitespace-pre-wrap">{line}</pre>
          <CopyButton text={line} />
        </li>
      ))}
    </ul>
  );
}

function TypeLink({id, children}: {id: string; children: ReactNode}) {
  const {registry, ctx} = useSession();
  if (!registry.get(id)) return <>{children}</>;
  return (
    <Link className="font-medium text-primary hover:underline" to={listUrl(ctx, id)}>
      {children}
    </Link>
  );
}

/** Content origin of the server (`http://host:port`), as clients reach it. */
export function useOrigin(): {origin: string | undefined; loading: boolean} {
  const {pulp, info, ctx} = useSession();
  const status = useQuery({queryKey: ["pulp", ctx, "status"], queryFn: () => pulp.get(info.status_path)});
  const settings = (status.data as Json | undefined)?.content_settings as {content_origin?: string} | undefined;
  return {origin: settings?.content_origin?.replace(/\/+$/, "") ?? (status.data ? info.base_url : undefined), loading: status.isLoading};
}

/** Distribution serving a repository, for its name/path. */
function useDistribution(plugin: string, repository: string | undefined): Json | undefined {
  const {registry, pulp, ctx} = useSession();
  const path = registry.get(`distributions/${plugin}/${plugin === "hugging_face" ? "hugging-face" : plugin}`)?.list_path;
  return useQuery({
    queryKey: ["pulp", ctx, "dist-of", repository],
    queryFn: async () => (await pulp.list(path!, {repository: repository!, limit: 1})).results[0] ?? null,
    enabled: !!path && !!repository,
  }).data as Json | undefined;
}

function containerWays(origin: string, repo: Json | undefined, repoKind: string | undefined, dist: Json | undefined): Way[] {
  const url = new URL(origin);
  const host = url.host;
  const insecure = url.protocol === "http:";
  const tls = insecure ? "--tls-verify=false " : "";
  const isPush = repoKind === "container-push";
  const name = (isPush && typeof dist?.base_path === "string" && dist.base_path) || (isPush && typeof repo?.name === "string" && repo.name) || "my-app";
  const image = `${host}/${name}:latest`;
  return [
    {
      title: "Push an image from your machine",
      recommended: true,
      text: (
        <>
          Pulp is a container registry: log in with your Pulp user, then push.{" "}
          {isPush ? (
            <>
              The image goes into this push repository (<span className="font-mono">{name}</span>).
            </>
          ) : (
            <>
              The first push to a name creates its <TypeLink id="repositories/container/container-push">push repository</TypeLink> and its
              distribution{repoKind === "container" ? "; a synced repository like this one does not receive pushes" : ""}.
            </>
          )}
        </>
      ),
      commands: [
        `podman login ${tls}${host}`,
        `podman tag <local-image> ${image}\npodman push ${tls}${image}`,
      ],
    },
    {
      title: "Copy an image from another registry",
      text: <>Without pulling it locally first, with skopeo (after the login above):</>,
      commands: [`skopeo copy ${insecure ? "--dest-tls-verify=false " : ""}docker://docker.io/library/alpine:latest docker://${host}/${isPush ? name : "alpine"}:latest`],
    },
    {
      title: "Mirror a repository of another registry",
      text: (
        <>
          Create a <TypeLink id="remotes/container/container">container remote</TypeLink> (URL{" "}
          <span className="font-mono">https://registry-1.docker.io</span>, upstream name such as{" "}
          <span className="font-mono">library/alpine</span>), then <b>Sync</b> a{" "}
          <TypeLink id="repositories/container/container">container repository</TypeLink> from it.
        </>
      ),
    },
    {
      title: "Cache images on demand",
      text: (
        <>
          A <TypeLink id="distributions/container/pull-through">pull-through distribution</TypeLink> (base path{" "}
          <span className="font-mono">docker-cache</span>, with a <TypeLink id="remotes/container/pull-through">pull-through remote</TypeLink>) stores
          every image pulled through it:
        </>
      ),
      commands: [`podman pull ${tls}${host}/docker-cache/library/alpine:latest`],
    },
    ...(repoKind === "container"
      ? [{title: "Build from a Containerfile", text: <>Advanced mode: <b>More actions › Build image</b> on this repository, with the Containerfile and its build context.</>}]
      : []),
    ...(insecure
      ? [
          {
            title: "Docker instead of Podman",
            text: (
              <>
                The server is in plain HTTP: Docker needs it declared in <span className="font-mono">/etc/docker/daemon.json</span>, then a
                restart of the daemon. Use the same commands with <span className="font-mono">docker</span> and without{" "}
                <span className="font-mono">--tls-verify</span>.
              </>
            ),
            commands: [`{"insecure-registries": ["${host}"]}`],
          },
        ]
      : []),
  ];
}

function huggingFaceWays(origin: string, dist: Json | undefined): Way[] {
  const basePath = typeof dist?.base_path === "string" ? dist.base_path : "<base-path>";
  const endpoint = typeof dist?.base_url === "string" ? dist.base_url.replace(/\/+$/, "") : `${origin}/pulp/content/${basePath}`;
  return [
    {
      title: "Cache models on demand",
      recommended: true,
      text: (
        <>
          Create a <TypeLink id="remotes/hugging_face/hugging-face">Hugging Face remote</TypeLink> (URL{" "}
          <span className="font-mono">https://huggingface.co</span>, with a token for private models), then a{" "}
          <TypeLink id="distributions/hugging_face/hugging-face">distribution</TypeLink> using it as <b>remote</b>. Point the Hugging Face client
          at the distribution: Pulp keeps every file it serves.
        </>
      ),
      commands: [`export HF_ENDPOINT=${endpoint}\nhuggingface-cli download <organization>/<model>`],
    },
  ];
}

export function AddContentHelp({
  plugin,
  repository,
  onClose,
}: {
  plugin: string;
  /** Repository the help is opened from, to fill in its name and address. */
  repository?: {href: string; item: Json};
  onClose: () => void;
}) {
  const {registry} = useSession();
  const {origin, loading} = useOrigin();
  const dist = useDistribution(plugin, repository?.href);
  const repoKind = repository ? registry.typeForHref(repository.href)?.name : undefined;
  const ways = !origin
    ? []
    : plugin === "container"
      ? containerWays(origin, repository?.item, repoKind, dist)
      : huggingFaceWays(origin, dist);
  const why =
    plugin === "container"
      ? "Container images are not uploaded as files: they are pushed with a registry client, copied, synced or cached."
      : "Hugging Face files are not uploaded as files: Pulp fetches them from the Hub when clients ask for them.";

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="flex max-h-[90vh] flex-col sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Lightbulb className="size-5 text-primary" />
            Add {pluginLabel(plugin)} content
          </DialogTitle>
          <DialogDescription>{why}</DialogDescription>
        </DialogHeader>
        <div className="-mx-4 flex min-h-0 flex-col gap-5 overflow-y-auto px-4">
          {loading && <Skeleton className="h-32 w-full" />}
          {ways.map((w, i) => (
            <section key={w.title} className="flex flex-col gap-2">
              <h3 className="flex items-center gap-2 text-sm font-semibold">
                <span className="flex size-5 items-center justify-center rounded-full bg-primary/10 text-xs text-primary">{i + 1}</span>
                {w.title}
                {w.recommended && <Badge variant="secondary">Recommended</Badge>}
              </h3>
              <p className="text-sm leading-relaxed text-muted-foreground">{w.text}</p>
              {w.commands && <Commands lines={w.commands} />}
            </section>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
