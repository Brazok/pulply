// `/storage`: where the disk goes.
//
// Built for a full production server: the top tiles only use `count` queries (instant at any
// size); the exact breakdown by plugin, content type, largest items and repository comes
// from a scan run by `pulply serve` in the background, started on demand and kept on disk.

import {useQuery} from "@tanstack/react-query";
import {ChevronRight, CircleAlert, HardDrive, Package, Play, RefreshCw, Square, Trash2} from "lucide-react";
import {type ReactNode, useState} from "react";
import {Link} from "react-router";

import {OrphanCleanupDialog, useCount, useRunning} from "../components/maintenance";
import {PageHeader} from "../components/PageHeader";
import {Badge} from "../components/ui/badge";
import {Button} from "../components/ui/button";
import {Card, CardContent, CardDescription, CardHeader, CardTitle} from "../components/ui/card";
import {Collapsible, CollapsibleContent, CollapsibleTrigger} from "../components/ui/collapsible";
import {Progress} from "../components/ui/progress";
import {Skeleton} from "../components/ui/skeleton";
import {Tooltip, TooltipContent, TooltipTrigger} from "../components/ui/tooltip";
import type {Json} from "../lib/api";
import {bytes, pluginLabel, relativeTime, typeTitle} from "../lib/format";
import {MAINTENANCE} from "../lib/maintenance";
import {adminUrl, base, itemUrl, listUrl} from "../lib/routes";
import {useSession} from "../lib/session";
import {type Report, type Usage, scanRatio, useStorageScan} from "../lib/storage";
import {cn} from "../lib/utils";

const pct = (part: number, whole: number) => (whole > 0 ? (part / whole) * 100 : 0);
const share = (part: number, whole: number) => {
  const p = pct(part, whole);
  return p > 0 && p < 1 ? "<1%" : `${Math.round(p)}%`;
};
const files = (n: number) => `${n.toLocaleString()} file${n === 1 ? "" : "s"}`;
const duration = (s: number) => (s < 60 ? `${s} s` : s < 3600 ? `${Math.round(s / 60)} min` : `${(s / 3600).toFixed(1)} h`);

function Tile({icon, label, value, detail, tone = "default", children}: {
  icon: ReactNode;
  label: string;
  value: ReactNode;
  detail?: ReactNode;
  tone?: "default" | "warning" | "danger";
  children?: ReactNode;
}) {
  const colors = {default: "bg-muted text-foreground", warning: "bg-warning/15 text-warning", danger: "bg-destructive/10 text-destructive"};
  return (
    <Card>
      <CardContent className="flex flex-col gap-3">
        <div className="flex items-center gap-3">
          <div className={cn("flex size-10 shrink-0 items-center justify-center rounded-xl [&_svg]:size-5", colors[tone])}>{icon}</div>
          <div className="flex min-w-0 flex-col">
            <span className="text-sm text-muted-foreground">{label}</span>
            <span className="text-2xl font-semibold tabular-nums">{value}</span>
          </div>
        </div>
        {detail && <div className="text-sm text-muted-foreground">{detail}</div>}
        {children}
      </CardContent>
    </Card>
  );
}

/** One row of a ranked bar list: label, bar (scaled to the largest row), size and share. */
function BarRow({label, usage, max, whole, muted, children}: {
  label: ReactNode;
  usage: Usage;
  max: number;
  whole: number;
  muted?: boolean;
  children?: ReactNode;
}) {
  return (
    <div className="grid grid-cols-[minmax(7rem,12rem)_1fr_auto] items-center gap-x-4 gap-y-1 py-1.5 max-sm:grid-cols-[1fr_auto]">
      <div className="min-w-0 truncate text-sm">{label}</div>
      <Tooltip>
        <TooltipTrigger asChild>
          <div className="flex h-5 items-center max-sm:order-last max-sm:col-span-2">
            <div
              className={cn("h-2.5 min-w-0.5 rounded-r-[4px]", muted ? "bg-muted-foreground/40" : "bg-primary")}
              style={{width: `${Math.max(pct(usage.size, max), 0.3)}%`}}
            />
          </div>
        </TooltipTrigger>
        <TooltipContent>
          {usage.size.toLocaleString()} bytes · {files(usage.count)} · {share(usage.size, whole)} of the artifacts
        </TooltipContent>
      </Tooltip>
      <div className="text-right text-sm whitespace-nowrap tabular-nums">
        <span className="font-medium">{bytes(usage.size)}</span>
        <span className="ms-2 inline-block w-10 text-muted-foreground">{share(usage.size, whole)}</span>
      </div>
      {children && <div className="col-span-full">{children}</div>}
    </div>
  );
}

function PluginRows({report}: {report: Report}) {
  const {ctx, registry} = useSession();
  const [open, setOpen] = useState<Set<string>>(new Set());
  const max = Math.max(...report.plugins.map((p) => p.size), report.orphaned.size, report.unattributed.size, 1);
  const whole = report.total.size;
  if (!report.plugins.length && !report.orphaned.count) return <p className="text-sm text-muted-foreground">No artifact is stored yet.</p>;
  const label = (id: string) => {
    const t = registry.get(id);
    return t ? typeTitle(t) : id;
  };
  return (
    <div className="flex flex-col divide-y">
      {report.plugins.map((p) => {
        const repoType = registry.pluginType("repositories", p.plugin, false);
        return (
          <Collapsible
            key={p.plugin}
            className="py-1"
            open={open.has(p.plugin)}
            onOpenChange={(o) =>
              setOpen((s) => {
                const next = new Set(s);
                if (o) next.add(p.plugin);
                else next.delete(p.plugin);
                return next;
              })
            }
          >
            <BarRow
              label={
                <span className="flex items-center gap-1.5">
                  <span className="truncate font-medium">{pluginLabel(p.plugin)}</span>
                  <span className="text-xs text-muted-foreground">{p.count.toLocaleString()}</span>
                </span>
              }
              max={max}
              usage={p}
              whole={whole}
            />
            {p.top.length > 0 && (
              <ol className="mb-1 flex flex-col gap-0.5 ps-3">
                {p.top.map((t, i) => (
                  <li key={t.href} className="flex items-center gap-2 text-xs">
                    <span className="w-3 text-right text-muted-foreground tabular-nums">{i + 1}</span>
                    <Link className="min-w-0 flex-1 truncate font-mono hover:text-primary hover:underline" title={t.label} to={itemUrl(ctx, t.href)}>
                      {t.label}
                    </Link>
                    <span className="text-muted-foreground tabular-nums">{bytes(t.size)}</span>
                  </li>
                ))}
              </ol>
            )}
            <CollapsibleTrigger className="group flex items-center gap-1 ps-3 text-xs text-muted-foreground hover:text-foreground">
              <ChevronRight className="size-3 transition-transform group-data-[state=open]:rotate-90" />
              By content type
            </CollapsibleTrigger>
            <CollapsibleContent className="mt-1 mb-2 ms-3 rounded-lg bg-muted/40 px-3 py-1">
              {p.types.map((t) => (
                <BarRow
                  key={t.id}
                  label={
                    <Link className="text-muted-foreground hover:text-foreground hover:underline" to={listUrl(ctx, t.id)}>
                      {label(t.id)}
                    </Link>
                  }
                  max={p.size}
                  usage={t}
                  whole={whole}
                />
              ))}
              {repoType && (
                <Link className="block py-1.5 text-xs text-primary hover:underline" to={listUrl(ctx, repoType.id)}>
                  {pluginLabel(p.plugin)} repositories →
                </Link>
              )}
            </CollapsibleContent>
          </Collapsible>
        );
      })}
      {report.orphaned.count > 0 && (
        <BarRow muted label={<span className="text-muted-foreground">Orphaned artifacts</span>} max={max} usage={report.orphaned} whole={whole} />
      )}
      {report.unattributed.count > 0 && (
        <BarRow muted label={<span className="text-muted-foreground">Other (unattributed)</span>} max={max} usage={report.unattributed} whole={whole} />
      )}
    </div>
  );
}

function RepoRows({report}: {report: Report}) {
  const {ctx} = useSession();
  const top = report.repositories.filter((r) => r.count > 0).slice(0, 15);
  if (!top.length) return <p className="text-sm text-muted-foreground">No repository holds files yet.</p>;
  const max = top[0].size || 1;
  return (
    <div className="flex flex-col divide-y">
      {top.map((r) => (
        <BarRow
          key={r.href}
          label={
            <span className="flex min-w-0 items-center gap-2">
              <Link className="truncate font-medium hover:underline" to={itemUrl(ctx, r.href)}>
                {r.name}
              </Link>
              <Badge className="shrink-0" variant="secondary">
                {pluginLabel(r.plugin)}
              </Badge>
            </span>
          }
          max={max}
          usage={r}
          whole={report.total.size}
        />
      ))}
    </div>
  );
}

function ScanPanel() {
  const {ctx, info} = useSession();
  const {state, start, cancel} = useStorageScan(ctx);
  const artifacts = useCount(`${info.api_prefix}artifacts/`);
  const content = useCount(`${info.api_prefix}content/`);
  const s = state.data;
  const report = s?.report;
  const rows = (artifacts ?? 0) * 2 + (content ?? 0);

  if (state.isLoading) return <Skeleton className="h-40 w-full" />;
  return (
    <>
      {s?.running && s.progress && (
        <Card>
          <CardContent className="flex flex-col gap-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-sm font-medium">
                Step {s.progress.step || 1}/{s.progress.steps || 5} · {s.progress.label || "Starting"}
                {scanRatio(s.progress) !== null && <span className="ms-2 text-muted-foreground tabular-nums">{Math.round(scanRatio(s.progress)! * 100)}%</span>}
              </span>
              <Button disabled={cancel.isPending} size="sm" variant="outline" onClick={() => cancel.mutate()}>
                <Square />
                Cancel
              </Button>
            </div>
            <Progress aria-label="Scan progress" value={scanRatio(s.progress) !== null ? scanRatio(s.progress)! * 100 : null} />
            <span className="text-xs text-muted-foreground">
              {s.progress.done.toLocaleString()} / {s.progress.total.toLocaleString()}
              {s.started_at && <> · started {relativeTime(s.started_at)}</>} · runs on the pulply server: you can leave this page.
            </span>
          </CardContent>
        </Card>
      )}
      {s?.error && !s.running && (
        <div className="flex items-center gap-2 rounded-lg bg-destructive/10 p-3 text-sm text-destructive">
          <CircleAlert className="size-4 shrink-0" />
          The last scan stopped: {s.error}
        </div>
      )}
      {!report && !s?.running && (
        <Card>
          <CardHeader>
            <CardTitle>Breakdown by plugin and repository</CardTitle>
            <CardDescription>
              Pulp keeps sizes on artifacts and does not aggregate them: the breakdown reads every artifact and content unit
              {artifacts !== undefined && content !== undefined && (
                <> (about {rows.toLocaleString()} rows, {Math.max(1, Math.ceil(rows / 1000)).toLocaleString()} requests)</>
              )}
              . The scan runs on the pulply server in the background, with read-only requests, and its result is kept.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Button disabled={start.isPending} onClick={() => start.mutate()}>
              <Play />
              Compute the breakdown
            </Button>
          </CardContent>
        </Card>
      )}
      {report && (
        <>
          <div className="flex flex-wrap items-center justify-between gap-2 text-sm text-muted-foreground">
            <span>
              Computed {relativeTime(report.computed_at)} in {duration(report.duration_secs)}
              {report.repositories_total > 0 && <> · {report.repositories_total.toLocaleString()} repositories</>}
            </span>
            {!s?.running && (
              <Button disabled={start.isPending} size="sm" variant="outline" onClick={() => start.mutate()}>
                <RefreshCw />
                Recompute
              </Button>
            )}
          </div>
          {report.warnings.length > 0 && (
            <div className="flex items-start gap-2 rounded-lg bg-warning/10 p-3 text-sm">
              <CircleAlert className="mt-0.5 size-4 shrink-0 text-warning" />
              <span>
                Not read (the server failed to list them), their files count as unattributed: {report.warnings.join("; ")}
              </span>
            </div>
          )}
          <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>By plugin</CardTitle>
                <CardDescription>Each file counts once, for the content using it. The 3 largest items of each plugin are listed.</CardDescription>
              </CardHeader>
              <CardContent>
                <PluginRows report={report} />
              </CardContent>
            </Card>
            <Card>
              <CardHeader>
                <CardTitle>Largest repositories</CardTitle>
                <CardDescription>Files of the latest version. A file shared by several repositories counts in each.</CardDescription>
              </CardHeader>
              <CardContent>
                <RepoRows report={report} />
              </CardContent>
            </Card>
          </div>
        </>
      )}
    </>
  );
}

export function StoragePage() {
  const {pulp, info, ctx, refresh, readonly, registry} = useSession();
  const [cleanup, setCleanup] = useState(false);
  const cleaning = useRunning(MAINTENANCE.orphans);
  const status = useQuery({queryKey: ["pulp", ctx, "status"], queryFn: () => pulp.get(info.status_path), refetchInterval: refresh * 3});
  const {state} = useStorageScan(ctx);
  const report = state.data?.report;
  const storage = (status.data as Json | undefined)?.storage as {total?: number; used?: number; free?: number} | undefined;
  const usedPct = storage?.total ? pct(storage.used ?? 0, storage.total) : 0;
  const artifacts = useCount(`${info.api_prefix}artifacts/`);
  const orphanArtifacts = useCount(`${info.api_prefix}artifacts/`, {orphaned_for: 0});
  const orphanContent = useCount(`${info.api_prefix}content/`, {orphaned_for: 0});
  const loading = <Skeleton className="mt-1 h-7 w-24" />;

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        crumbs={[{label: "Home", to: base(ctx)}, {label: "Administration", to: adminUrl(ctx)}, {label: "Disk usage"}]}
        subtitle="Where the server's storage goes: by plugin, content type, item and repository."
        title="Disk usage"
      />

      <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
        <Tile
          detail={storage?.total ? `${bytes(storage.free)} free of ${bytes(storage.total)}` : "Not reported by this server"}
          icon={<HardDrive />}
          label="Disk used"
          tone={usedPct > 90 ? "danger" : usedPct > 75 ? "warning" : "default"}
          value={status.isLoading ? loading : storage?.total ? bytes(storage.used) : "—"}
        >
          {storage?.total ? (
            <Progress aria-label="Disk used" className={cn("h-2", usedPct > 90 && "*:data-[slot=progress-indicator]:bg-destructive")} value={usedPct} />
          ) : null}
        </Tile>
        <Tile
          detail={report ? `${files(report.total.count)} at the last scan` : "Files stored by Pulp"}
          icon={<Package />}
          label="Artifacts"
          value={report ? bytes(report.total.size) : artifacts === undefined ? loading : files(artifacts)}
        />
        <Tile
          detail={
            orphanArtifacts === undefined ? undefined : orphanArtifacts || orphanContent ? (
              <>
                {files(orphanArtifacts)} and {(orphanContent ?? 0).toLocaleString()} content units no repository uses.{" "}
                {MAINTENANCE.orphans in registry.operations && !readonly ? (
                  <Button className="h-auto p-0" disabled={cleaning} variant="link" onClick={() => setCleanup(true)}>
                    {cleaning ? "Cleanup running…" : "Clean up…"}
                  </Button>
                ) : (
                  <Link className="text-primary hover:underline" to={`${base(ctx)}/operations`}>
                    Maintenance…
                  </Link>
                )}
              </>
            ) : (
              "Nothing to clean up."
            )
          }
          icon={<Trash2 />}
          label="Reclaimable"
          tone={orphanArtifacts ? "warning" : "default"}
          value={report ? bytes(report.orphaned.size) : orphanArtifacts === undefined ? loading : files(orphanArtifacts)}
        />
      </div>

      <ScanPanel />
      <OrphanCleanupDialog isOpen={cleanup} onOpenChange={setCleanup} />
    </div>
  );
}
