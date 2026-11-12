// Server-wide operations (`orphans/cleanup/`, `repair/`, `tasks/purge/`, `rpm/copy/`…).

import {Button, Card, Chip, Skeleton} from "@heroui/react";
import {useQuery} from "@tanstack/react-query";
import {useState} from "react";

import {PageHeader} from "../components/PageHeader";
import {type FormSpec, FormModal} from "../components/SchemaForm";
import {pluginLabel, titleize} from "../lib/format";
import type {GlobalOperation} from "../lib/registry";
import {base} from "../lib/routes";
import {useSession} from "../lib/session";

interface Known {
  title: string;
  description: string;
  danger: boolean;
}

const KNOWN: Record<string, Known> = {
  "orphans/cleanup/": {
    title: "Clean up orphans",
    description: "Delete content and artifacts that no repository version references any more.",
    danger: true,
  },
  "repositories/reclaim_space/": {
    title: "Reclaim disk space",
    description: "Remove the artifacts of repositories; their content becomes on-demand.",
    danger: true,
  },
  "repair/": {
    title: "Repair storage",
    description: "Check every artifact and re-download missing or corrupted files.",
    danger: false,
  },
  "tasks/purge/": {
    title: "Purge finished tasks",
    description: "Delete finished task records older than a date.",
    danger: true,
  },
};

function plugin(rel: string, plugins: string[]): string {
  const segs = rel.split("/");
  return segs.find((s) => plugins.includes(s)) ?? "core";
}

/** OpenAPI summary as a title: no trailing period. */
function title(rel: string, op: GlobalOperation): string {
  return KNOWN[rel]?.title ?? (op.summary ?? titleize(rel.replace(/\/$/, ""))).replace(/\.$/, "");
}

export function OperationsPage() {
  const {registry, ctx, info, pulp, refresh} = useSession();
  const [form, setForm] = useState<FormSpec | null>(null);
  const plugins = registry.plugins();
  const orphans = useQuery({
    queryKey: ["pulp", ctx, "orphans-count"],
    queryFn: async () => (await pulp.list(`${info.api_prefix}content/`, {orphaned_for: 0, limit: 1, fields: "pulp_href"})).count,
    refetchInterval: refresh * 6,
  });
  const ops = Object.entries(registry.operations).sort(([a], [b]) => Number(!(a in KNOWN)) - Number(!(b in KNOWN)) || a.localeCompare(b));

  const open = (rel: string, op: GlobalOperation) => {
    const known = KNOWN[rel];
    setForm({
      title: title(rel, op),
      label: title(rel, op),
      method: "POST",
      path: op.path,
      schema: registry.schema(op.body_schema),
      plugin: plugin(rel, plugins),
      multipart: op.multipart,
      danger: known?.danger,
      showResult: true,
      submitLabel: "Run",
      intro: known ? <p className="text-sm text-muted">{known.description}</p> : undefined,
    });
  };

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        crumbs={[{label: "Home", to: base(ctx)}, {label: "Maintenance"}]}
        subtitle="Server-wide operations. Each one asks for its parameters before running."
        title="Maintenance"
      />
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
        {ops.map(([rel, op]) => {
          const known = KNOWN[rel];
          const p = plugin(rel, plugins);
          return (
            <Card key={rel} className="flex flex-col">
              <Card.Header>
                <div className="flex items-center gap-2">
                  <Card.Title>{title(rel, op)}</Card.Title>
                  {p !== "core" && (
                    <Chip size="sm" variant="soft">
                      {pluginLabel(p)}
                    </Chip>
                  )}
                </div>
                <Card.Description>{known?.description ?? <span className="font-mono text-xs">{rel}</span>}</Card.Description>
              </Card.Header>
              <Card.Content className="flex-1">
                {rel === "orphans/cleanup/" &&
                  (orphans.data === undefined ? (
                    <Skeleton className="h-5 w-40 rounded-lg" />
                  ) : (
                    <p className="text-sm">
                      <span className="font-semibold">{orphans.data.toLocaleString()}</span> orphaned content units right now
                    </p>
                  ))}
              </Card.Content>
              <Card.Footer>
                <Button isDisabled={info.readonly} variant={known?.danger ? "danger-soft" : "secondary"} onPress={() => open(rel, op)}>
                  Run…
                </Button>
              </Card.Footer>
            </Card>
          );
        })}
      </div>
      <FormModal spec={form} onClose={() => setForm(null)} />
    </div>
  );
}
