import {Search} from "lucide-react";
import {useMemo, useState} from "react";

import {PageHeader} from "../components/PageHeader";
import {Avatar, AvatarFallback} from "../components/ui/avatar";
import {Badge} from "../components/ui/badge";
import {Card, CardContent, CardDescription, CardHeader, CardTitle} from "../components/ui/card";
import {InputGroup, InputGroupAddon, InputGroupInput} from "../components/ui/input-group";
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
        <CardContent className="flex flex-row items-center gap-4">
          <Avatar className="size-12">
            <AvatarFallback className="bg-primary/10 text-primary">{(info.username ?? "?").slice(0, 2).toUpperCase()}</AvatarFallback>
          </Avatar>
          <div className="flex flex-col gap-1">
            <span className="text-lg font-semibold">{info.username ?? "anonymous"}</span>
            <span className="text-sm text-muted-foreground">
              {info.base_url} · context {ctx}
            </span>
            {href && <HrefLink href={href} label="Open my user" />}
          </div>
          <div className="ms-auto flex gap-2">
            {info.readonly && <Badge variant="warning">read-only</Badge>}
            <Badge variant={info.access === "all" ? "success" : "info"}>access: {info.access}</Badge>
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>Permissions</CardTitle>
          <CardDescription>{ACCESS[info.access]}</CardDescription>
        </CardHeader>
        {info.access === "limited" && (
          <CardContent className="flex flex-col gap-3">
            <InputGroup className="max-w-sm">
              <InputGroupAddon>
                <Search />
              </InputGroupAddon>
              <InputGroupInput aria-label="Filter permissions" placeholder="Filter…" type="search" value={q} onChange={(e) => setQ(e.target.value)} />
            </InputGroup>
            <div className="flex flex-wrap gap-1.5">
              {perms.map((p) => (
                <Badge key={p} variant="secondary">
                  {p}
                </Badge>
              ))}
              {!perms.length && <span className="text-sm text-muted-foreground">No global permission.</span>}
            </div>
          </CardContent>
        )}
      </Card>
    </div>
  );
}
