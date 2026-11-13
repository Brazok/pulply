// Server-wide operations. The usual maintenance (orphan cleanup, task purge, storage repair,
// space reclaim) runs in one click with sensible presets; every operation keeps its full
// form under "Options…", and the other global operations (`rpm/copy/`, `rpm/prune/`…) follow.

import {CalendarX2, HardDriveDownload, ShieldCheck, Trash2} from "lucide-react";
import {type ReactNode, useState} from "react";

import {ConfirmDialog} from "../components/dialogs";
import {LastRun, OrphanCleanupDialog, orphanText, useCount, useOrphans, useRunning, useRunOperation} from "../components/maintenance";
import {PageHeader} from "../components/PageHeader";
import {type FormSpec, FormModal} from "../components/SchemaForm";
import {Badge} from "../components/ui/badge";
import {Button} from "../components/ui/button";
import {Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle} from "../components/ui/card";
import {Skeleton} from "../components/ui/skeleton";
import {ToggleGroup, ToggleGroupItem} from "../components/ui/toggle-group";
import {pluginLabel, titleize} from "../lib/format";
import {cutoff, MAINTENANCE, type MaintenanceOp, operationGroup, PURGE_AGES, purgeStates} from "../lib/maintenance";
import type {GlobalOperation} from "../lib/registry";
import {adminUrl, base} from "../lib/routes";
import {useSession} from "../lib/session";

function plugin(rel: string, plugins: string[]): string {
  return rel.split("/").find((s) => plugins.includes(s)) ?? "core";
}

/** OpenAPI summary as a title: no trailing period. */
function title(rel: string, op: GlobalOperation): string {
  return (op.summary ?? titleize(rel.replace(/\/$/, ""))).replace(/\.$/, "");
}

type OpenForm = (rel: string, overrides?: Partial<FormSpec>) => void;

function OpCard({icon, title, description, rel, children, actions}: {
  icon: ReactNode;
  title: string;
  description: string;
  rel: MaintenanceOp;
  children?: ReactNode;
  actions: ReactNode;
}) {
  return (
    <Card className="flex flex-col">
      <CardHeader>
        <div className="flex items-center gap-3">
          <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted [&_svg]:size-4.5">{icon}</div>
          <CardTitle>{title}</CardTitle>
        </div>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-1 flex-col gap-3">
        {children}
        <LastRun rel={rel} />
      </CardContent>
      <CardFooter className="flex flex-wrap gap-2">{actions}</CardFooter>
    </Card>
  );
}

function OptionsButton({rel, openForm}: {rel: string; openForm: OpenForm}) {
  return (
    <Button className="ms-auto" variant="ghost" onClick={() => openForm(rel)}>
      Options…
    </Button>
  );
}

function OrphansCard({openForm}: {openForm: OpenForm}) {
  const {readonly} = useSession();
  const orphans = useOrphans();
  const running = useRunning(MAINTENANCE.orphans);
  const [confirm, setConfirm] = useState(false);
  return (
    <OpCard
      actions={
        <>
          <Button disabled={readonly || running || orphans.none} variant="destructive" onClick={() => setConfirm(true)}>
            Clean up…
          </Button>
          <OptionsButton openForm={openForm} rel={MAINTENANCE.orphans} />
          <OrphanCleanupDialog isOpen={confirm} onOpenChange={setConfirm} />
        </>
      }
      description="Delete content and files that no repository version uses any more."
      icon={<Trash2 />}
      rel={MAINTENANCE.orphans}
      title="Clean up orphans"
    >
      {orphans.content === undefined || orphans.artifacts === undefined ? (
        <Skeleton className="h-5 w-56" />
      ) : orphans.none ? (
        <p className="text-sm text-muted-foreground">Nothing to clean up.</p>
      ) : (
        <p className="text-sm">
          <span className="font-semibold">{orphanText(orphans)}</span> to clean up.
        </p>
      )}
    </OpCard>
  );
}

function PurgeCard({openForm}: {openForm: OpenForm}) {
  const {registry, info, readonly} = useSession();
  const run = useRunOperation();
  const running = useRunning(MAINTENANCE.purge);
  const [days, setDays] = useState(30);
  const [confirm, setConfirm] = useState(false);
  const op = registry.operations[MAINTENANCE.purge];
  const statesProp = registry.schema(op?.body_schema)?.properties?.states;
  const states = purgeStates(statesProp ? registry.enumChoices(statesProp) : []);
  const before = cutoff(days);
  const count = useCount(`${info.api_prefix}tasks/`, {finished_at__lt: before, state__in: states.join(",")});
  const tasks = `${(count ?? 0).toLocaleString()} finished task${count === 1 ? "" : "s"}`;
  return (
    <OpCard
      actions={
        <>
          <Button disabled={readonly || running || count === 0} variant="destructive" onClick={() => setConfirm(true)}>
            Purge…
          </Button>
          <OptionsButton openForm={openForm} rel={MAINTENANCE.purge} />
          <ConfirmDialog
            confirmLabel="Purge"
            isOpen={confirm}
            title="Purge finished tasks"
            danger
            onConfirm={() => run(MAINTENANCE.purge, {finished_before: before, states}, "Task purge")}
            onOpenChange={setConfirm}
          >
            Deletes the records of {tasks} ({states.join(", ")}) finished before {before}. Their content and repository
            versions are kept.
          </ConfirmDialog>
        </>
      }
      description="Delete the records of old finished tasks: the task list stays short and fast."
      icon={<CalendarX2 />}
      rel={MAINTENANCE.purge}
      title="Purge finished tasks"
    >
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <span className="text-muted-foreground">Older than</span>
        <ToggleGroup
          aria-label="Age of the tasks to purge"
          className="rounded-lg border bg-muted/40 p-0.5"
          size="sm"
          type="single"
          value={String(days)}
          onValueChange={(v) => v && setDays(Number(v))}
        >
          {PURGE_AGES.map((d) => (
            <ToggleGroupItem key={d} className="px-2.5 data-[state=on]:bg-background data-[state=on]:shadow-sm" value={String(d)}>
              {d < 365 ? `${d} days` : "1 year"}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>
      {count === undefined ? (
        <Skeleton className="h-5 w-40" />
      ) : (
        <p className="text-sm">{count ? <><span className="font-semibold">{tasks}</span> to purge.</> : <span className="text-muted-foreground">Nothing to purge.</span>}</p>
      )}
    </OpCard>
  );
}

function RepairCard() {
  const {readonly} = useSession();
  const run = useRunOperation();
  const running = useRunning(MAINTENANCE.repair);
  const [confirm, setConfirm] = useState(false);
  const [pending, setPending] = useState(false);
  const quick = async () => {
    setPending(true);
    try {
      await run(MAINTENANCE.repair, {verify_checksums: false}, "Storage check");
    } finally {
      setPending(false);
    }
  };
  return (
    <OpCard
      actions={
        <>
          <Button disabled={readonly || running || pending} variant="outline" onClick={() => void quick()}>
            Check files
          </Button>
          <Button disabled={readonly || running} variant="outline" onClick={() => setConfirm(true)}>
            Verify checksums…
          </Button>
          <ConfirmDialog
            confirmLabel="Verify"
            isOpen={confirm}
            title="Verify every stored file"
            onConfirm={() => run(MAINTENANCE.repair, {verify_checksums: true}, "Storage repair")}
            onOpenChange={setConfirm}
          >
            Reads every stored file and compares its checksum, then downloads again the missing or corrupted ones. This
            can take hours and loads the disk on a large server.
          </ConfirmDialog>
        </>
      }
      description="Find missing or corrupted files and download them again from their remote."
      icon={<ShieldCheck />}
      rel={MAINTENANCE.repair}
      title="Repair storage"
    >
      <p className="text-xs text-muted-foreground">
        <span className="font-medium text-foreground">Check files</span> only looks for missing files (fast).{" "}
        <span className="font-medium text-foreground">Verify checksums</span> also reads them (slow, finds corruption).
      </p>
    </OpCard>
  );
}

function ReclaimCard({openForm}: {openForm: OpenForm}) {
  const {readonly} = useSession();
  const run = useRunOperation();
  const running = useRunning(MAINTENANCE.reclaim);
  const [confirm, setConfirm] = useState(false);
  return (
    <OpCard
      actions={
        <>
          <Button disabled={readonly || running} variant="outline" onClick={() => openForm(MAINTENANCE.reclaim, {title: "Reclaim disk space", submitLabel: "Reclaim"})}>
            Choose repositories…
          </Button>
          <Button disabled={readonly || running} variant="destructive" onClick={() => setConfirm(true)}>
            All repositories…
          </Button>
          <ConfirmDialog
            confirmLabel="Reclaim"
            isOpen={confirm}
            title="Reclaim the space of every repository"
            danger
            onConfirm={() => run(MAINTENANCE.reclaim, {repo_hrefs: ["*"]}, "Space reclaim")}
            onOpenChange={setConfirm}
          >
            Deletes the stored files of every repository that Pulp can download again from a remote. They are then served
            on demand: the remote must stay reachable. Content without a remote (uploads) keeps its files.
          </ConfirmDialog>
        </>
      }
      description="Delete stored files that can be downloaded again from a remote; they become on-demand."
      icon={<HardDriveDownload />}
      rel={MAINTENANCE.reclaim}
      title="Reclaim disk space"
    />
  );
}

function GenericCard({rel, op, plugins, openForm}: {rel: string; op: GlobalOperation; plugins: string[]; openForm: OpenForm}) {
  const {readonly} = useSession();
  const p = plugin(rel, plugins);
  return (
    <Card className="flex flex-col">
      <CardHeader>
        <div className="flex items-center gap-2">
          <CardTitle>{title(rel, op)}</CardTitle>
          {p !== "core" && <Badge variant="secondary">{pluginLabel(p)}</Badge>}
        </div>
        <CardDescription className="font-mono text-xs">{rel}</CardDescription>
      </CardHeader>
      <CardFooter className="mt-auto">
        <Button disabled={readonly} variant="outline" onClick={() => openForm(rel)}>
          Run…
        </Button>
      </CardFooter>
    </Card>
  );
}

function Section({title, description, children}: {title: string; description: string; children: ReactNode}) {
  return (
    <section className="flex flex-col gap-3">
      <div>
        <h2 className="text-lg font-semibold">{title}</h2>
        <p className="text-sm text-muted-foreground">{description}</p>
      </div>
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">{children}</div>
    </section>
  );
}

export function OperationsPage() {
  const {registry, ctx} = useSession();
  const [form, setForm] = useState<FormSpec | null>(null);
  const plugins = registry.plugins();
  const ops = registry.operations;
  const has = (rel: string) => rel in ops;
  const rest = Object.entries(ops).sort(([a], [b]) => a.localeCompare(b));
  const maintenance = rest.filter(([rel]) => operationGroup(rel) === "maintenance");
  const other = rest.filter(([rel]) => operationGroup(rel) === "other");

  const openForm: OpenForm = (rel, overrides) => {
    const op = ops[rel];
    if (!op) return;
    setForm({
      title: title(rel, op),
      label: title(rel, op),
      method: "POST",
      path: op.path,
      schema: registry.schema(op.body_schema),
      plugin: plugin(rel, plugins),
      multipart: op.multipart,
      danger: operationGroup(rel) !== "other" && rel !== MAINTENANCE.repair,
      showResult: true,
      submitLabel: "Run",
      ...overrides,
    });
  };

  return (
    <div className="flex flex-col gap-8">
      <PageHeader
        crumbs={[{label: "Home", to: base(ctx)}, {label: "Administration", to: adminUrl(ctx)}, {label: "Maintenance"}]}
        subtitle="Keep the server clean and healthy. The usual cases run in one click; Options… gives every parameter."
        title="Maintenance"
      />
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        {has(MAINTENANCE.orphans) && <OrphansCard openForm={openForm} />}
        {has(MAINTENANCE.purge) && <PurgeCard openForm={openForm} />}
        {has(MAINTENANCE.repair) && <RepairCard />}
        {has(MAINTENANCE.reclaim) && <ReclaimCard openForm={openForm} />}
      </div>
      {maintenance.length > 0 && (
        <Section description="Plugin-specific cleanups and repairs." title="More maintenance">
          {maintenance.map(([rel, op]) => (
            <GenericCard key={rel} op={op} openForm={openForm} plugins={plugins} rel={rel} />
          ))}
        </Section>
      )}
      {other.length > 0 && (
        <Section description="Server-wide operations on content: copies between repositories, imports…" title="Content operations">
          {other.map(([rel, op]) => (
            <GenericCard key={rel} op={op} openForm={openForm} plugins={plugins} rel={rel} />
          ))}
        </Section>
      )}
      <FormModal spec={form} onClose={() => setForm(null)} />
    </div>
  );
}
