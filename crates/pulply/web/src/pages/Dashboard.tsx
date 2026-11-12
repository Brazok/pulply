// Server overview: task activity, workers, storage, versions and quick actions.

import {CirclePlay, CircleXmark, Clock, CloudArrowUpIn, Cpu, Database, Layers, Plus, Server} from "@gravity-ui/icons";
import {Button, Card, Chip, Dropdown, Label, Meter, Skeleton} from "@heroui/react";
import {useQuery} from "@tanstack/react-query";
import {type ReactNode, useState} from "react";
import {Link, useNavigate} from "react-router";

import {PageHeader} from "../components/PageHeader";
import {type FormSpec, FormModal} from "../components/SchemaForm";
import {UploadDialog} from "../components/UploadDialog";
import {StateChip, TimeAgo} from "../components/values";
import type {Json} from "../lib/api";
import {bytes, duration, pluginLabel, singular, taskName, typeTitle} from "../lib/format";
import {itemUrl, listUrl} from "../lib/routes";
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
    default: "bg-default text-foreground",
    accent: "bg-accent-soft text-accent",
    danger: "bg-danger/15 text-danger",
    success: "bg-success/15 text-success",
    warning: "bg-warning/15 text-warning",
  };
  const body = (
    <Card className="h-full transition-shadow hover:shadow-md">
      <Card.Content className="flex flex-row items-center gap-4">
        <div className={`flex size-11 shrink-0 items-center justify-center rounded-2xl ${colors[tone]}`}>{icon}</div>
        <div className="flex min-w-0 flex-col">
          <span className="text-sm text-muted">{label}</span>
          {value === undefined ? (
            <Skeleton className="mt-1 h-6 w-12 rounded-lg" />
          ) : (
            <span className="text-2xl font-semibold tabular-nums">{typeof value === "number" ? value.toLocaleString() : value}</span>
          )}
        </div>
      </Card.Content>
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
              <Button variant="secondary" onPress={() => setUpload(true)}>
                <CloudArrowUpIn />
                Upload
              </Button>
            )}
            {repoTypes.length > 0 && (
              <Dropdown>
                <Button>
                  <Plus />
                  New repository
                </Button>
                <Dropdown.Popover placement="bottom end">
                  <Dropdown.Menu onAction={(k) => createRepo(String(k))}>
                    {repoTypes.map((t) => (
                      <Dropdown.Item key={t!.id} id={t!.id} textValue={pluginLabel(t!.plugin)}>
                        <Label>{pluginLabel(t!.plugin)}</Label>
                      </Dropdown.Item>
                    ))}
                  </Dropdown.Menu>
                </Dropdown.Popover>
              </Dropdown>
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
        <Stat icon={<Clock className="size-5" />} label="Waiting tasks" to={listUrl(ctx, "tasks", {state: "waiting"})} value={waiting} />
        <Stat
          icon={<CircleXmark className="size-5" />}
          label="Failed in the last 24h"
          to={listUrl(ctx, "tasks", {state: "failed"})}
          tone={failed ? "danger" : "success"}
          value={failed}
        />
        <Stat icon={<Cpu className="size-5" />} label="Workers online" to={listUrl(ctx, "workers")} tone={workers.length ? "success" : "warning"} value={s ? workers.length : undefined} />
        <Stat icon={<Database className="size-5" />} label="Repositories" to={listUrl(ctx, "repositories")} value={repos} />
        <Stat icon={<Server className="size-5" />} label="Distributions" to={listUrl(ctx, "distributions")} value={dists} />
        <Stat icon={<Layers className="size-5" />} label="Content units" to={listUrl(ctx, "content")} value={contentCount} />
        <Stat icon={<Server className="size-5" />} label="Content apps online" tone={apps.length ? "success" : "warning"} value={s ? apps.length : undefined} />
      </div>

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-3">
        <Card className="xl:col-span-2">
          <Card.Header className="flex flex-row items-center justify-between">
            <Card.Title>Recent tasks</Card.Title>
            <Link className="text-sm text-accent hover:underline" to={listUrl(ctx, "tasks")}>
              View all
            </Link>
          </Card.Header>
          <Card.Content className="flex flex-col divide-y divide-separator">
            {recent.isLoading && [0, 1, 2].map((i) => <Skeleton key={i} className="my-2 h-8 rounded-lg" />)}
            {recent.data?.results.length === 0 && <p className="py-6 text-center text-sm text-muted">No task yet.</p>}
            {recent.data?.results.map((t) => (
              <Link
                key={t.pulp_href as string}
                className="flex items-center gap-3 py-2.5 hover:bg-default/40 -mx-2 px-2 rounded-lg"
                to={itemUrl(ctx, t.pulp_href as string)}
              >
                <StateChip state={t.state} />
                <span className="flex-1 truncate text-sm font-medium" title={String(t.name)}>
                  {taskName(t.name)}
                </span>
                <span className="hidden text-xs text-muted sm:block">
                  {duration(t.started_at as string, (t.finished_at as string) || null)}
                </span>
                <span className="w-28 text-right text-xs text-muted">
                  <TimeAgo ts={t.pulp_created as string} />
                </span>
              </Link>
            ))}
          </Card.Content>
        </Card>

        <div className="flex flex-col gap-4">
          {storage?.total ? (
            <Card>
              <Card.Header>
                <Card.Title>Storage</Card.Title>
              </Card.Header>
              <Card.Content className="flex flex-col gap-2">
                <Meter aria-label="Storage used" color={(storage.used ?? 0) / storage.total > 0.9 ? "danger" : "accent"} maxValue={storage.total} value={storage.used ?? 0}>
                  <Meter.Track>
                    <Meter.Fill />
                  </Meter.Track>
                </Meter>
                <div className="flex justify-between text-sm">
                  <span>{bytes(storage.used)} used</span>
                  <span className="text-muted">{bytes(storage.free)} free</span>
                </div>
              </Card.Content>
            </Card>
          ) : null}
          <Card>
            <Card.Header className="flex flex-row items-center justify-between">
              <Card.Title>Server</Card.Title>
              {db !== undefined && (
                <Chip color={db ? "success" : "danger"} size="sm" variant="soft">
                  database {db ? "connected" : "down"}
                </Chip>
              )}
            </Card.Header>
            <Card.Content>
              <ul className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-sm">
                {versions.map((v) => (
                  <li key={v.component} className="flex justify-between gap-2">
                    <span className="truncate text-muted">{pluginLabel(v.component)}</span>
                    <span className="font-mono text-xs">{v.version}</span>
                  </li>
                ))}
              </ul>
            </Card.Content>
          </Card>
        </div>
      </div>

      <FormModal spec={form} onClose={() => setForm(null)} />
      {upload && <UploadDialog onClose={() => setUpload(false)} />}
    </div>
  );
}
