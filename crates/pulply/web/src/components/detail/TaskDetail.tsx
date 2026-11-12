// A task followed live: progress reports, timings, error, created resources, sub-tasks.

import {CircleStop} from "@gravity-ui/icons";
import {Button, Card, Label, ProgressBar, Spinner} from "@heroui/react";
import {useQuery} from "@tanstack/react-query";
import {useState} from "react";

import type {Json} from "../../lib/api";
import {dateTime, duration, taskName} from "../../lib/format";
import {base, listUrl} from "../../lib/routes";
import {errorText, useSession} from "../../lib/session";
import {ConfirmDialog} from "../dialogs";
import {PageHeader} from "../PageHeader";
import {ResourceList} from "../ResourceList";
import {HrefLink, JsonView, PrnLink, StateChip, TimeAgo} from "../values";

const ACTIVE = new Set(["waiting", "running", "canceling"]);

export function TaskDetail({href}: {href: string}) {
  const {ctx, refresh, pulp, registry, info} = useSession();
  // Polled while the task is active.
  const task = useQuery({
    queryKey: ["pulp", ctx, href, null],
    queryFn: () => pulp.get<Json>(href),
    refetchInterval: (q) => (ACTIVE.has(String(q.state.data?.state ?? "waiting")) ? Math.min(refresh, 2000) : false),
  });
  const [cancel, setCancel] = useState(false);
  const t = task.data;
  if (task.isLoading) return <Spinner className="m-10" />;
  if (task.error || !t) return <p className="text-sm text-danger">{errorText(task.error)}</p>;

  const state = String(t.state);
  const reports = (t.progress_reports as Json[] | undefined) ?? [];
  const created = (t.created_resources as string[] | undefined) ?? [];
  const reserved = ((t.reserved_resources_record as string[] | undefined) ?? []).map((r) => r.replace(/^shared:/, ""));
  const error = t.error as Json | null;
  const tasks = registry.get("tasks");

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        actions={
          ACTIVE.has(state) &&
          !info.readonly && (
            <Button variant="danger-soft" onPress={() => setCancel(true)}>
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
          <Card.Header className="flex flex-row items-center justify-between">
            <Card.Title>Progress</Card.Title>
            {ACTIVE.has(state) && <Spinner size="sm" />}
          </Card.Header>
          <Card.Content className="flex flex-col gap-4">
            {!reports.length && <p className="text-sm text-muted">{state === "waiting" ? "Waiting for a worker…" : "No progress report."}</p>}
            {reports.map((r, i) => {
              const total = Number(r.total ?? 0);
              const done = Number(r.done ?? 0);
              return (
                <ProgressBar
                  key={`${r.code}-${i}`}
                  color={r.state === "failed" ? "danger" : r.state === "completed" ? "success" : "accent"}
                  isIndeterminate={!total && r.state === "running"}
                  maxValue={total || 1}
                  value={total ? done : r.state === "completed" ? 1 : 0}
                  valueLabel={total ? `${done.toLocaleString()} / ${total.toLocaleString()}` : `${done.toLocaleString()}`}
                >
                  <Label>{String(r.message)}</Label>
                  <ProgressBar.Output />
                  <ProgressBar.Track>
                    <ProgressBar.Fill />
                  </ProgressBar.Track>
                </ProgressBar>
              );
            })}
            {error && (
              <div className="flex flex-col gap-2 rounded-xl bg-danger/10 p-3">
                <p className="text-sm font-medium text-danger">{String(error.description ?? "Task failed")}</p>
                {error.traceback ? (
                  <pre className="max-h-80 overflow-auto font-mono text-xs leading-5">{String(error.traceback)}</pre>
                ) : null}
              </div>
            )}
          </Card.Content>
        </Card>

        <Card>
          <Card.Header>
            <Card.Title>Timing</Card.Title>
          </Card.Header>
          <Card.Content>
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
              <dt className="text-muted">Created</dt>
              <dd title={dateTime(t.pulp_created as string)}>
                <TimeAgo ts={t.pulp_created as string} />
              </dd>
              <dt className="text-muted">Started</dt>
              <dd>{t.started_at ? <TimeAgo ts={t.started_at as string} /> : "—"}</dd>
              <dt className="text-muted">Finished</dt>
              <dd>{t.finished_at ? <TimeAgo ts={t.finished_at as string} /> : "—"}</dd>
              <dt className="text-muted">Duration</dt>
              <dd>{duration(t.started_at as string, (t.finished_at as string) || null) || "—"}</dd>
              <dt className="text-muted">Worker</dt>
              <dd>{t.worker ? <HrefLink href={t.worker as string} /> : "—"}</dd>
              {t.parent_task ? (
                <>
                  <dt className="text-muted">Parent</dt>
                  <dd>
                    <HrefLink href={t.parent_task as string} />
                  </dd>
                </>
              ) : null}
              {t.task_group ? (
                <>
                  <dt className="text-muted">Group</dt>
                  <dd>
                    <HrefLink href={t.task_group as string} />
                  </dd>
                </>
              ) : null}
            </dl>
          </Card.Content>
        </Card>
      </div>

      {(created.length > 0 || reserved.length > 0) && (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          {created.length > 0 && (
            <Card>
              <Card.Header>
                <Card.Title>Created resources</Card.Title>
              </Card.Header>
              <Card.Content className="flex flex-col gap-1">
                {created.map((h) => (
                  <HrefLink key={h} href={h} />
                ))}
              </Card.Content>
            </Card>
          )}
          {reserved.length > 0 && (
            <Card>
              <Card.Header>
                <Card.Title>Resources</Card.Title>
              </Card.Header>
              <Card.Content className="flex flex-col gap-1">
                {reserved.map((r) =>
                  r.startsWith("/") ? (
                    <HrefLink key={r} href={r} />
                  ) : r.startsWith("prn:core.domain:") ? null : r.startsWith("prn:") ? (
                    <PrnLink key={r} prn={r} />
                  ) : (
                    <span key={r} className="font-mono text-xs text-muted">
                      {r}
                    </span>
                  ),
                )}
              </Card.Content>
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

      <details className="rounded-2xl bg-surface p-4">
        <summary className="cursor-pointer text-sm font-medium">Raw task</summary>
        <div className="mt-3">
          <JsonView value={t} />
        </div>
      </details>

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
