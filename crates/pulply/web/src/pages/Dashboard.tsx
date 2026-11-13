// Server overview: task activity, workers, storage, versions and quick actions.

import {CirclePlay, CircleX, Clock, CloudUpload, Cpu, Database, Layers, Plus, Server} from "lucide-react";
import {useQuery} from "@tanstack/react-query";
import {type ReactNode, useState} from "react";
import {Link, useNavigate} from "react-router";

import {PageHeader} from "../components/PageHeader";
import {type FormSpec, FormModal} from "../components/SchemaForm";
import {Badge} from "../components/ui/badge";
import {Button} from "../components/ui/button";
import {Card, CardAction, CardContent, CardHeader, CardTitle} from "../components/ui/card";
import {DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger} from "../components/ui/dropdown-menu";
import {Progress} from "../components/ui/progress";
import {Skeleton} from "../components/ui/skeleton";
import {UploadDialog} from "../components/UploadDialog";
import {StateChip, TimeAgo} from "../components/values";
import type {Json} from "../lib/api";
import {bytes, duration, pluginLabel, singular, taskName, typeTitle} from "../lib/format";
import {base, itemUrl, listUrl} from "../lib/routes";
import {useMode} from "../lib/mode";
import {useSession} from "../lib/session";

function Stat({
  label,
  value,
  icon,
  tone = "default",
  to,
}: {
  label: string;
  value: number | string | undefined;
  icon: ReactNode;
  tone?: "default" | "accent" | "danger" | "success" | "warning";
  to?: string;
}) {
  const colors = {
    default: "bg-muted text-foreground",
    accent: "bg-primary/10 text-primary",
    danger: "bg-destructive/10 text-destructive",
    success: "bg-success/15 text-success",
    warning: "bg-warning/15 text-warning",
  };
  const body = (
    <Card className="h-full transition-colors hover:bg-muted/40">
      <CardContent className="flex flex-row items-center gap-4">
        <div className={`flex size-11 shrink-0 items-center justify-center rounded-xl ${colors[tone]}`}>{icon}</div>
        <div className="flex min-w-0 flex-col">
          <span className="text-sm text-muted-foreground">{label}</span>
          {value === undefined ? (
            <Skeleton className="mt-1 h-6 w-12" />
          ) : (
            <span className="text-2xl font-semibold tabular-nums">{typeof value === "number" ? value.toLocaleString() : value}</span>
          )}
        </div>
      </CardContent>
    </Card>
  );
  return to ? (
    <Link className="block" to={to}>
      {body}
    </Link>
  ) : (
    body
  );
}

function Health({label, ok, text}: {label: string; ok: boolean; text: string | undefined}) {
  return (
    <li className="flex items-center justify-between gap-2">
      <span className="flex items-center gap-2">
        <span className={`size-2 rounded-full ${ok ? "bg-success" : "bg-warning"}`} />
        {label}
      </span>
      {text === undefined ? <Skeleton className="h-4 w-16" /> : <span className="text-muted-foreground">{text}</span>}
    </li>
  );
}

/** `count` of a list endpoint. */
function useCount(path: string, params: Record<string, string> = {}, interval?: number): number | undefined {
  const {pulp, ctx} = useSession();
  return useQuery({
    queryKey: ["pulp", ctx, "count", path, params],
    queryFn: async () => (await pulp.list(path, {...params, limit: 1, fields: "pulp_href"})).count,
    refetchInterval: interval,
  }).data;
}

export function Dashboard() {
  const {pulp, info, ctx, refresh, registry, can} = useSession();
  const {simple} = useMode();
  const navigate = useNavigate();
  const [form, setForm] = useState<FormSpec | null>(null);
  const [upload, setUpload] = useState(false);
  const status = useQuery({
    queryKey: ["pulp", ctx, "status"],
    queryFn: () => pulp.get(info.status_path),
    refetchInterval: refresh * 3,
  });
  const tasks = info.api_prefix + "tasks/";
  // Rounded to the minute so the query key stays stable between renders.
  const since = new Date(Math.floor(Date.now() / 60_000) * 60_000 - 24 * 3600 * 1000).toISOString();
  const running = useCount(tasks, {state: "running"}, refresh);
  const waiting = useCount(tasks, {state: "waiting"}, refresh);
  const failed = useCount(tasks, {state: "failed", finished_at__gte: since}, refresh * 3);
  const repos = useCount(info.api_prefix + "repositories/");
  const dists = useCount(info.api_prefix + "distributions/");
  const contentCount = useCount(info.api_prefix + "content/");
  const recent = useQuery({
    queryKey: ["pulp", ctx, "recent-tasks"],
    queryFn: () =>
      pulp.list(tasks, {
        limit: 8,
        ordering: "-pulp_created",
        fields: "pulp_href,name,state,pulp_created,started_at,finished_at",
      }),
    refetchInterval: refresh,
  });

  const s = status.data as Json | undefined;
  const workers = (s?.online_workers as Json[] | undefined) ?? [];
  const apps = (s?.online_content_apps as Json[] | undefined) ?? [];
  const storage = s?.storage as {total?: number; used?: number; free?: number} | undefined;
  const versions = (s?.versions as {component: string; version: string; package?: string}[] | undefined) ?? info.versions;
  const db = (s?.database_connection as Json | undefined)?.connected;

  const repoTypes = registry.plugins().map((p) => registry.pluginType("repositories", p)).filter((t) => !!t && can("add", t));
  const createRepo = (id: string) => {
    const t = registry.get(id)!;
    setForm({
      title: `New ${singular(typeTitle(t)).toLowerCase()}`,
      label: `Create ${singular(typeTitle(t)).toLowerCase()}`,
      method: "POST",
      path: t.list_path!,
      schema: registry.schema(t.create_schema),
      plugin: t.plugin,
      multipart: t.create_multipart,
      submitLabel: "Create",
      onDone: (resp) => {
        const href = (resp as Json)?.pulp_href;
        if (typeof href === "string") navigate(itemUrl(ctx, href));
      },
    });
  };

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        actions={
          <>
            {registry.uploadTypes().length > 0 && !info.readonly && (
              <Button variant="outline" onClick={() => setUpload(true)}>
                <CloudUpload />
                Upload
              </Button>
            )}
            {repoTypes.length > 0 && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button>
                    <Plus />
                    New repository
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  {repoTypes.map((t) => (
                    <DropdownMenuItem key={t!.id} onSelect={() => createRepo(t!.id)}>
                      {pluginLabel(t!.plugin)}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuContent>
              </DropdownMenu>
            )}
          </>
        }
        subtitle={
          <>
            {info.base_url}
            {info.username && <> · signed in as {info.username}</>}
          </>
        }
        title={`Welcome to ${ctx}`}
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <Stat icon={<CirclePlay className="size-5" />} label="Running tasks" to={listUrl(ctx, "tasks", {state: "running"})} tone="accent" value={running} />
        {!simple && <Stat icon={<Clock className="size-5" />} label="Waiting tasks" to={listUrl(ctx, "tasks", {state: "waiting"})} value={waiting} />}
        <Stat
          icon={<CircleX className="size-5" />}
          label="Failed in the last 24h"
          to={listUrl(ctx, "tasks", {state: "failed"})}
          tone={failed ? "danger" : "success"}
          value={failed}
        />
        {!simple && <Stat icon={<Cpu className="size-5" />} label="Workers online" to={listUrl(ctx, "workers")} tone={workers.length ? "success" : "warning"} value={s ? workers.length : undefined} />}
        <Stat icon={<Database className="size-5" />} label="Repositories" to={listUrl(ctx, "repositories")} value={repos} />
        <Stat icon={<Server className="size-5" />} label="Distributions" to={listUrl(ctx, "distributions")} value={dists} />
        {!simple && (
          <>
            <Stat icon={<Layers className="size-5" />} label="Content units" to={listUrl(ctx, "content")} value={contentCount} />
            <Stat icon={<Server className="size-5" />} label="Content apps online" tone={apps.length ? "success" : "warning"} value={s ? apps.length : undefined} />
          </>
        )}
      </div>

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-3">
        <Card className="xl:col-span-2">
          <CardHeader>
            <CardTitle>Recent tasks</CardTitle>
            <CardAction>
              <Link className="text-sm text-primary hover:underline" to={listUrl(ctx, "tasks")}>
                View all
              </Link>
            </CardAction>
          </CardHeader>
          <CardContent className="flex flex-col divide-y">
            {recent.isLoading && [0, 1, 2].map((i) => <Skeleton key={i} className="my-2 h-8" />)}
            {recent.data?.results.length === 0 && <p className="py-6 text-center text-sm text-muted-foreground">No task yet.</p>}
            {recent.data?.results.map((t) => (
              <Link
                key={t.pulp_href as string}
                className="-mx-2 flex items-center gap-3 rounded-md px-2 py-2.5 hover:bg-muted/60"
                to={itemUrl(ctx, t.pulp_href as string)}
              >
                <StateChip state={t.state} />
                <span className="flex-1 truncate text-sm font-medium" title={String(t.name)}>
                  {taskName(t.name)}
                </span>
                <span className="hidden text-xs text-muted-foreground sm:block">
                  {duration(t.started_at as string, (t.finished_at as string) || null)}
                </span>
                <span className="w-28 text-right text-xs text-muted-foreground">
                  <TimeAgo ts={t.pulp_created as string} />
                </span>
              </Link>
            ))}
          </CardContent>
        </Card>

        <div className="flex flex-col gap-4">
          {storage?.total ? (
            <Card>
              <CardHeader className="flex items-center justify-between">
                <CardTitle>Storage</CardTitle>
                <Link className="text-sm text-primary hover:underline" to={`${base(ctx)}/storage`}>
                  Details →
                </Link>
              </CardHeader>
              <CardContent className="flex flex-col gap-2">
                <Progress
                  aria-label="Storage used"
                  className={`h-2 ${(storage.used ?? 0) / storage.total > 0.9 ? "*:data-[slot=progress-indicator]:bg-destructive" : ""}`}
                  value={((storage.used ?? 0) / storage.total) * 100}
                />
                <div className="flex justify-between text-sm">
                  <span>{bytes(storage.used)} used</span>
                  <span className="text-muted-foreground">{bytes(storage.free)} free</span>
                </div>
              </CardContent>
            </Card>
          ) : null}
          <Card>
            <CardHeader>
              <CardTitle>Server</CardTitle>
              {db !== undefined && (
                <CardAction>
                  <Badge variant={db ? "success" : "destructive"}>database {db ? "connected" : "down"}</Badge>
                </CardAction>
              )}
            </CardHeader>
            <CardContent>
              {simple ? (
                <ul className="flex flex-col gap-2 text-sm">
                  <Health label="Workers" ok={workers.length > 0} text={s ? `${workers.length} online` : undefined} />
                  <Health label="Content apps" ok={apps.length > 0} text={s ? `${apps.length} online` : undefined} />
                  <Health label="Pulp" ok text={info.versions.find((v) => v.component === "core")?.version} />
                </ul>
              ) : (
                <ul className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-sm">
                  {versions.map((v) => (
                    <li key={v.component} className="flex justify-between gap-2">
                      <span className="truncate text-muted-foreground">{pluginLabel(v.component)}</span>
                      <span className="font-mono text-xs">{v.version}</span>
                    </li>
                  ))}
                </ul>
              )}
            </CardContent>
          </Card>
        </div>
      </div>

      <FormModal spec={form} onClose={() => setForm(null)} />
      {upload && <UploadDialog onClose={() => setUpload(false)} />}
    </div>
  );
}
