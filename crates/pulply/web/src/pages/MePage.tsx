import {Avatar, Card, Chip, SearchField} from "@heroui/react";
import {useMemo, useState} from "react";

import {PageHeader} from "../components/PageHeader";
import {HrefLink} from "../components/values";
import type {Json} from "../lib/api";
import {base} from "../lib/routes";
import {usePulp, useSession} from "../lib/session";

const ACCESS: Record<string, string> = {
  all: "Everything is offered (administrator, or permission filtering disabled). The server still has the last word.",
  limited: "Actions are offered according to your global permissions and your roles on each object.",
  objects: "Your global permissions are not readable: actions are offered according to your roles on each object.",
};

export function MePage() {
  const {info, ctx} = useSession();
  const me = usePulp<Json>(`${info.api_prefix}login/`);
  const [q, setQ] = useState("");
  const perms = useMemo(() => info.permissions.filter((p) => p.includes(q.toLowerCase())), [info.permissions, q]);
  const href = me.data?.pulp_href as string | undefined;
  return (
    <div className="flex flex-col gap-6">
      <PageHeader crumbs={[{label: "Home", to: base(ctx)}, {label: "My account"}]} title="My account" />
      <Card>
        <Card.Content className="flex flex-row items-center gap-4">
          <Avatar size="lg" variant="soft">
            <Avatar.Fallback>{(info.username ?? "?").slice(0, 2).toUpperCase()}</Avatar.Fallback>
          </Avatar>
          <div className="flex flex-col gap-1">
            <span className="text-lg font-semibold">{info.username ?? "anonymous"}</span>
            <span className="text-sm text-muted">
              {info.base_url} · context {ctx}
            </span>
            {href && <HrefLink href={href} label="Open my user" />}
          </div>
          <div className="ms-auto flex gap-2">
            {info.readonly && (
              <Chip color="warning" variant="soft">
                read-only
              </Chip>
            )}
            <Chip color={info.access === "all" ? "success" : "accent"} variant="soft">
              access: {info.access}
            </Chip>
          </div>
        </Card.Content>
      </Card>
      <Card>
        <Card.Header>
          <Card.Title>Permissions</Card.Title>
          <Card.Description>{ACCESS[info.access]}</Card.Description>
        </Card.Header>
        {info.access === "limited" && (
          <Card.Content className="flex flex-col gap-3">
            <SearchField aria-label="Filter permissions" className="max-w-sm" value={q} onChange={setQ}>
              <SearchField.Group>
                <SearchField.SearchIcon />
                <SearchField.Input placeholder="Filter…" />
                <SearchField.ClearButton />
              </SearchField.Group>
            </SearchField>
            <div className="flex flex-wrap gap-1.5">
              {perms.map((p) => (
                <Chip key={p} size="sm" variant="secondary">
                  {p}
                </Chip>
              ))}
              {!perms.length && <span className="text-sm text-muted">No global permission.</span>}
            </div>
          </Card.Content>
        )}
      </Card>
    </div>
  );
}
