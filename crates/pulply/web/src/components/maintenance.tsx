// Building blocks of the maintenance operations, shared by the Maintenance and Disk usage
// pages: running an operation in one click, its last run, and the orphan cleanup dialog.

import {useQuery, useQueryClient} from "@tanstack/react-query";
import {useState} from "react";
import {Link} from "react-router";
import {toast} from "sonner";

import type {Json} from "../lib/api";
import {bytes, relativeTime, TASK_COLORS} from "../lib/format";
import {isActive, MAINTENANCE, type MaintenanceOp, TASK_NAMES} from "../lib/maintenance";
import {itemUrl} from "../lib/routes";
import {useSession} from "../lib/session";
import {useStorageScan} from "../lib/storage";
import {ConfirmDialog} from "./dialogs";
import {Badge} from "./ui/badge";
import {Label} from "./ui/label";
import {Skeleton} from "./ui/skeleton";
import {Spinner} from "./ui/spinner";
import {Switch} from "./ui/switch";

/** `count` of a list endpoint (one row fetched: instant on any server). */
export function useCount(path: string | undefined, params: Record<string, string | number> = {}, refetchInterval?: number) {
  const {pulp, ctx} = useSession();
  return useQuery({
    queryKey: ["pulp", ctx, "count", path, params],
    queryFn: async () => (await pulp.list(path!, {...params, limit: 1, fields: "pulp_href"})).count,
    enabled: !!path,
    retry: false,
    refetchInterval,
  }).data;
}

/** Runs the global operation `rel` with `body` and follows its task. */
export function useRunOperation() {
  const {registry, pulp, track, ctx} = useSession();
  const queryClient = useQueryClient();
  return async (rel: string, body: Json, label: string) => {
    const op = registry.operations[rel];
    if (!op) throw new Error(`This server has no ${rel} operation.`);
    const response = await pulp.post(op.path, body);
    // The last run of the operation shows the new task at once.
    void queryClient.invalidateQueries({queryKey: ["pulp", ctx]});
    if (!track(response, label)) toast.success(`${label} done`);
  };
}

/** Latest task of a maintenance operation (any user), polled while it runs. */
export function useLastRun(rel: MaintenanceOp) {
  const {pulp, ctx, info, refresh} = useSession();
  return useQuery({
    queryKey: ["pulp", ctx, "last-run", rel],
    queryFn: async () =>
      (
        await pulp.list(`${info.api_prefix}tasks/`, {
          name__contains: TASK_NAMES[rel],
          ordering: "-pulp_created",
          limit: 1,
          fields: "pulp_href,state,pulp_created,started_at,finished_at",
        })
      ).results[0] ?? null,
    refetchInterval: (q) => (isActive(q.state.data?.state) ? 2000 : refresh * 6),
    retry: false,
  });
}

/** "Last run 3 days ago · completed", or "Running…" with a link to the task. */
export function LastRun({rel}: {rel: MaintenanceOp}) {
  const {ctx} = useSession();
  const last = useLastRun(rel);
  if (last.isLoading) return <Skeleton className="h-5 w-48" />;
  // Listing tasks may be forbidden: the card works without it.
  if (last.isError) return null;
  const task = last.data;
  if (!task) return <span className="text-xs text-muted-foreground">Never run on this server.</span>;
  const state = String(task.state);
  const link = (text: string) => (
    <Link className="hover:text-foreground hover:underline" to={itemUrl(ctx, String(task.pulp_href))}>
      {text}
    </Link>
  );
  if (isActive(state)) {
    return (
      <span className="flex items-center gap-2 text-xs text-info">
        <Spinner className="size-3.5" />
        {link(`${state === "waiting" ? "Waiting to start" : "Running"} · started ${relativeTime(String(task.started_at ?? task.pulp_created))}`)}
      </span>
    );
  }
  return (
    <span className="flex items-center gap-2 text-xs text-muted-foreground">
      {link(`Last run ${relativeTime(String(task.finished_at ?? task.pulp_created))}`)}
      <Badge variant={TASK_COLORS[state] ?? "secondary"}>{state}</Badge>
    </span>
  );
}

/** The operation is running: its quick actions wait for it. */
export function useRunning(rel: MaintenanceOp): boolean {
  return isActive(useLastRun(rel).data?.state);
}

/** Orphans right now: content units and artifacts no repository version uses. */
export function useOrphans() {
  const {info, refresh, ctx} = useSession();
  const content = useCount(`${info.api_prefix}content/`, {orphaned_for: 0}, refresh * 6);
  const artifacts = useCount(`${info.api_prefix}artifacts/`, {orphaned_for: 0}, refresh * 6);
  const size = useStorageScan(ctx).state.data?.report?.orphaned.size;
  return {content, artifacts, size, none: content === 0 && artifacts === 0};
}

/** Text of the orphan counts: "12 content units and 30 files (1.2 GB at the last scan)". */
export function orphanText({content, artifacts, size}: ReturnType<typeof useOrphans>): string {
  const parts = [
    `${(content ?? 0).toLocaleString()} content unit${content === 1 ? "" : "s"}`,
    `${(artifacts ?? 0).toLocaleString()} file${artifacts === 1 ? "" : "s"}`,
  ];
  return `${parts.join(" and ")}${size ? ` (${bytes(size)} at the last scan)` : ""}`;
}

/** Confirmation of the orphan cleanup, with the counts and the protection delay choice. */
export function OrphanCleanupDialog({isOpen, onOpenChange}: {isOpen: boolean; onOpenChange: (open: boolean) => void}) {
  const run = useRunOperation();
  const orphans = useOrphans();
  const [now, setNow] = useState(false);
  return (
    <ConfirmDialog
      confirmLabel="Clean up"
      isOpen={isOpen}
      title="Clean up orphans"
      danger
      onConfirm={() => run(MAINTENANCE.orphans, now ? {orphan_protection_time: 0} : {}, "Orphan cleanup")}
      onOpenChange={(open) => {
        if (!open) setNow(false);
        onOpenChange(open);
      }}
    >
      <p>
        Deletes {orphanText(orphans)} that no repository version uses any more. Repositories and their versions are not
        touched.
      </p>
      <div className="mt-3 flex items-start gap-3 rounded-lg border p-3 text-left">
        <Switch checked={now} id="orphans-now" onCheckedChange={setNow} />
        <Label className="flex flex-col items-start gap-1 font-normal" htmlFor="orphans-now">
          <span className="font-medium text-foreground">Include recent orphans</span>
          <span className="text-xs text-muted-foreground">
            By default, Pulp keeps orphans created recently (protection delay of the server) as a running task may still
            need them. Only include them when no sync or upload is running.
          </span>
        </Label>
      </div>
    </ConfirmDialog>
  );
}
