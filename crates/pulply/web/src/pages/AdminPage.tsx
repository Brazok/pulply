// `/admin`: entry point of the administration area, one block per section (access control,
// server, storage) with a card per page and the number of objects it holds.

import {useQuery} from "@tanstack/react-query";
import {ChevronRight} from "lucide-react";
import {useMemo} from "react";
import {Link} from "react-router";

import {PageHeader} from "../components/PageHeader";
import {adminItemIcon, adminSectionIcon} from "../components/adminIcons";
import {Badge} from "../components/ui/badge";
import {Skeleton} from "../components/ui/skeleton";
import {ADMIN_DESCRIPTIONS, type NavItem, buildNav} from "../lib/nav";
import {base} from "../lib/routes";
import {useSession} from "../lib/session";

/** Number of objects of a listed type; `null` when the user may not list it. */
function Count({id}: {id: string}) {
  const {registry, pulp, ctx} = useSession();
  const path = registry.get(id)?.list_path;
  const count = useQuery({
    queryKey: ["pulp", ctx, "count", path, {}],
    queryFn: async () => (await pulp.list(path!, {limit: 1, fields: "pulp_href"})).count,
    enabled: !!path,
    retry: false,
  });
  if (!path) return null;
  if (count.isError) return <span className="text-xs text-muted-foreground">no access</span>;
  if (count.data === undefined) return <Skeleton className="h-5 w-8" />;
  return <span className="text-lg font-semibold tabular-nums">{count.data.toLocaleString()}</span>;
}

function AdminCard({item}: {item: NavItem}) {
  return (
    <Link
      className="group flex items-start gap-3 rounded-xl border bg-card p-4 transition-colors hover:border-primary/40 hover:bg-muted/40"
      to={item.to}
    >
      <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted text-foreground [&_svg]:size-4.5">
        {adminItemIcon(item.id)}
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="flex items-center gap-1 font-medium">
          {item.label}
          <ChevronRight className="size-3.5 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" />
        </span>
        {ADMIN_DESCRIPTIONS[item.id] && <span className="text-sm text-muted-foreground">{ADMIN_DESCRIPTIONS[item.id]}</span>}
      </div>
      <Count id={item.id} />
    </Link>
  );
}

export function AdminPage() {
  const {registry, ctx, info} = useSession();
  const sections = useMemo(() => buildNav(registry, ctx).admin, [registry, ctx]);
  return (
    <div className="flex flex-col gap-8">
      <PageHeader
        chips={info.readonly && <Badge variant="warning">Read-only: browse only</Badge>}
        crumbs={[{label: "Home", to: base(ctx)}, {label: "Administration"}]}
        subtitle="Accounts and permissions, workers and storage of the server."
        title="Administration"
      />
      {sections.map((s) => (
        <section key={s.id} aria-labelledby={`admin-${s.id}`} className="flex flex-col gap-3">
          <div className="flex items-center gap-3">
            <div className="flex size-8 items-center justify-center rounded-lg bg-primary/10 text-primary [&_svg]:size-4">
              {adminSectionIcon(s.id)}
            </div>
            <div className="flex flex-col">
              <h2 className="text-base font-semibold" id={`admin-${s.id}`}>
                {s.label}
              </h2>
              <p className="text-sm text-muted-foreground">{s.description}</p>
            </div>
          </div>
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {s.items.map((i) => (
              <AdminCard key={i.id} item={i} />
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
