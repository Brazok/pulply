// A task followed live: progress reports, timings, error, created resources, sub-tasks.

import {useQuery} from "@tanstack/react-query";
import {ChevronRight, CircleStop} from "lucide-react";
import {useState} from "react";

import type {Json} from "../../lib/api";
import {dateTime, duration, taskName} from "../../lib/format";
import {useMode} from "../../lib/mode";
import {base, listUrl} from "../../lib/routes";
import {cn} from "../../lib/utils";
import {errorText, useSession} from "../../lib/session";
import {ConfirmDialog} from "../dialogs";
import {PageHeader} from "../PageHeader";
import {ResourceList} from "../ResourceList";
import {Button} from "../ui/button";
import {Card, CardAction, CardContent, CardHeader, CardTitle} from "../ui/card";
import {Collapsible, CollapsibleContent, CollapsibleTrigger} from "../ui/collapsible";
import {Progress} from "../ui/progress";
import {Spinner} from "../ui/spinner";
import {HrefLink, JsonView, PrnLink, StateChip, TimeAgo} from "../values";

const ACTIVE = new Set(["waiting", "running", "canceling"]);

export function TaskDetail({href}: {href: string}) {
  const {ctx, refresh, pulp, registry, info} = useSession();
  const {simple} = useMode();
  // Polled while the task is active.
  const task = useQuery({
    queryKey: ["pulp", ctx, href, null],
    queryFn: () => pulp.get<Json>(href),
    refetchInterval: (q) => (ACTIVE.has(String(q.state.data?.state ?? "waiting")) ? Math.min(refresh, 2000) : false),
  });
  const [cancel, setCancel] = useState(false);
  const t = task.data;
  if (task.isLoading) return <Spinner className="m-10 size-6" />;
  if (task.error || !t) return <p className="text-sm text-destructive">{errorText(task.error)}</p>;

  const state = String(t.state);
  const reports = (t.progress_reports as Json[] | undefined) ?? [];
  const created = (t.created_resources as string[] | undefined) ?? [];
  // Locks taken by the task: bookkeeping, shown in advanced mode.
  const reserved = simple ? [] : ((t.reserved_resources_record as string[] | undefined) ?? []).map((r) => r.replace(/^shared:/, ""));
  const error = t.error as Json | null;
  const tasks = registry.get("tasks");

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        actions={
          ACTIVE.has(state) &&
          !info.readonly && (
            <Button variant="destructive" onClick={() => setCancel(true)}>
              <CircleStop />
              Cancel task
            </Button>
          )
        }
        chips={<StateChip state={state} />}
        crumbs={[{label: "Home", to: base(ctx)}, {label: "Tasks", to: listUrl(ctx, "tasks")}, {label: taskName(t.name)}]}
        subtitle={<span className="font-mono text-xs">{String(t.name)}</span>}
        title={taskName(t.name)}
      />

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <Card className="lg:col-span-2">
          <CardHeader>
            <CardTitle>Progress</CardTitle>
            {ACTIVE.has(state) && (
              <CardAction>
                <Spinner className="text-muted-foreground" />
              </CardAction>
            )}
          </CardHeader>
          <CardContent className="flex flex-col gap-4">
            {!reports.length && <p className="text-sm text-muted-foreground">{state === "waiting" ? "Waiting for a worker…" : "No progress report."}</p>}
            {reports.map((r, i) => {
              const total = Number(r.total ?? 0);
              const done = Number(r.done ?? 0);
              return (
                <div key={`${r.code}-${i}`} className="flex flex-col gap-1.5">
                  <div className="flex items-center justify-between gap-3 text-sm">
                    <span className="font-medium">{String(r.message)}</span>
                    <span className="tabular-nums text-muted-foreground">
                      {total ? `${done.toLocaleString()} / ${total.toLocaleString()}` : done.toLocaleString()}
                    </span>
                  </div>
                  <Progress
                    aria-label={String(r.message)}
                    className={cn(
                      "h-2",
                      r.state === "failed" && "*:data-[slot=progress-indicator]:bg-destructive",
                      r.state === "completed" && "*:data-[slot=progress-indicator]:bg-success",
                    )}
                    value={!total && r.state === "running" ? null : total ? Math.min(100, (done / total) * 100) : r.state === "completed" ? 100 : 0}
                  />
                </div>
              );
            })}
            {error && (
              <div className="flex flex-col gap-2 rounded-lg bg-destructive/10 p-3">
                <p className="text-sm font-medium text-destructive">{String(error.description ?? "Task failed")}</p>
                {error.traceback ? (
                  <pre className="max-h-80 overflow-auto font-mono text-xs leading-5">{String(error.traceback)}</pre>
                ) : null}
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Timing</CardTitle>
          </CardHeader>
          <CardContent>
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
              <dt className="text-muted-foreground">Created</dt>
              <dd title={dateTime(t.pulp_created as string)}>
                <TimeAgo ts={t.pulp_created as string} />
              </dd>
              <dt className="text-muted-foreground">Started</dt>
              <dd>{t.started_at ? <TimeAgo ts={t.started_at as string} /> : "—"}</dd>
              <dt className="text-muted-foreground">Finished</dt>
              <dd>{t.finished_at ? <TimeAgo ts={t.finished_at as string} /> : "—"}</dd>
              <dt className="text-muted-foreground">Duration</dt>
              <dd>{duration(t.started_at as string, (t.finished_at as string) || null) || "—"}</dd>
              <dt className="text-muted-foreground">Worker</dt>
              <dd>{t.worker ? <HrefLink href={t.worker as string} /> : "—"}</dd>
              {t.parent_task ? (
                <>
                  <dt className="text-muted-foreground">Parent</dt>
                  <dd>
                    <HrefLink href={t.parent_task as string} />
                  </dd>
                </>
              ) : null}
              {t.task_group ? (
                <>
                  <dt className="text-muted-foreground">Group</dt>
                  <dd>
                    <HrefLink href={t.task_group as string} />
                  </dd>
                </>
              ) : null}
            </dl>
          </CardContent>
        </Card>
      </div>

      {(created.length > 0 || reserved.length > 0) && (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          {created.length > 0 && (
            <Card>
              <CardHeader>
                <CardTitle>Created resources</CardTitle>
              </CardHeader>
              <CardContent className="flex flex-col gap-1">
                {created.map((h) => (
                  <HrefLink key={h} href={h} />
                ))}
              </CardContent>
            </Card>
          )}
          {reserved.length > 0 && (
            <Card>
              <CardHeader>
                <CardTitle>Resources</CardTitle>
              </CardHeader>
              <CardContent className="flex flex-col gap-1">
                {reserved.map((r) =>
                  r.startsWith("/") ? (
                    <HrefLink key={r} href={r} />
                  ) : r.startsWith("prn:core.domain:") ? null : r.startsWith("prn:") ? (
                    <PrnLink key={r} prn={r} />
                  ) : (
                    <span key={r} className="font-mono text-xs text-muted-foreground">
                      {r}
                    </span>
                  ),
                )}
              </CardContent>
            </Card>
          )}
        </div>
      )}

      {tasks && (t.child_tasks as string[] | undefined)?.length ? (
        <div className="flex flex-col gap-2">
          <h2 className="text-lg font-semibold">Sub-tasks</h2>
          <ResourceList embedded baseQuery={{parent_task: href}} path={tasks.list_path!} type={tasks} />
        </div>
      ) : null}

      {!simple && (
        <Collapsible className="rounded-xl border bg-card p-4">
          <CollapsibleTrigger className="group flex w-full items-center gap-2 text-sm font-medium">
            <ChevronRight className="size-4 transition-transform group-data-[state=open]:rotate-90" />
            Raw task
          </CollapsibleTrigger>
          <CollapsibleContent className="mt-3">
            <JsonView value={t} />
          </CollapsibleContent>
        </Collapsible>
      )}

      <ConfirmDialog
        danger
        confirmLabel="Cancel task"
        isOpen={cancel}
        title="Cancel this task?"
        onConfirm={async () => {
          await pulp.patch(href, {state: "canceled"});
          await task.refetch();
        }}
        onOpenChange={setCancel}
      />
    </div>
  );
}
