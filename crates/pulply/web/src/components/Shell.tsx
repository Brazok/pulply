// Application frame: sidebar navigation, top bar (context, search, running tasks,
// theme, account) and the command palette (Ctrl/⌘ K).

import {useQuery, useQueryClient} from "@tanstack/react-query";
import {
  BookCheck,
  Boxes,
  Check,
  CloudDownload,
  Database,
  FileInput,
  FileOutput,
  HardDrive,
  Network,
  Send,
  ShieldCheck,
  ChevronDown,
  ChevronRight,
  Clock,
  House,
  ListChecks,
  Menu,
  Monitor,
  Moon,
  RefreshCw,
  Search,
  Settings,
  Sun,
  User,
} from "lucide-react";
import {type ReactNode, useEffect, useMemo, useRef, useState} from "react";
import {NavLink, useLocation, useNavigate} from "react-router";
import {toast} from "sonner";
import {Command as CommandPrimitive} from "cmdk";

import {type Json, postJson} from "../lib/api";
import {categoryLabel, relativeTime, taskName, typeTitle} from "../lib/format";
import {ADMIN_DESCRIPTIONS, type AdminSection, type NavItem, allDestinations, buildNav, simplifyNav} from "../lib/nav";
import {adminUrl, base, hrefOfPath, itemUrl, listUrl} from "../lib/routes";
import {errorText, useContexts, useSession} from "../lib/session";
import {useMode} from "../lib/mode";
import {scanRatio, useStorageScan} from "../lib/storage";
import {useTheme} from "../lib/theme";
import {cn} from "../lib/utils";
import {adminItemIcon} from "./adminIcons";
import {Logo} from "./screens";
import {Avatar, AvatarFallback} from "./ui/avatar";
import {Badge} from "./ui/badge";
import {Button} from "./ui/button";
import {Collapsible, CollapsibleContent, CollapsibleTrigger} from "./ui/collapsible";
import {Dialog, DialogContent, DialogTitle} from "./ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "./ui/dropdown-menu";
import {Kbd, KbdGroup} from "./ui/kbd";
import {Popover, PopoverContent, PopoverTrigger} from "./ui/popover";
import {Progress} from "./ui/progress";
import {Sheet, SheetContent, SheetHeader, SheetTitle} from "./ui/sheet";
import {Spinner} from "./ui/spinner";
import {ToggleGroup, ToggleGroupItem} from "./ui/toggle-group";
import {Tooltip, TooltipContent, TooltipTrigger} from "./ui/tooltip";

function NavEntry({item, icon, onNavigate, title}: {item: NavItem; icon?: ReactNode; onNavigate?: () => void; title?: string}) {
  const location = useLocation();
  return (
    <NavLink
      className={({isActive}) => {
        // Exact match: `All repositories` must not light up on `File repositories`.
        const active = isActive || location.pathname === `${item.to}/`;
        return cn(
          "flex items-center gap-2.5 rounded-md px-3 py-1.5 text-sm transition-colors [&_svg]:size-4 [&_svg]:shrink-0",
          active
            ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
            : "text-sidebar-foreground/80 hover:bg-sidebar-accent/60 hover:text-sidebar-accent-foreground",
        );
      }}
      end
      title={title}
      to={item.to}
      onClick={onNavigate}
    >
      {icon}
      <span className="truncate">{item.label}</span>
    </NavLink>
  );
}

/** Cross-plugin lists of the sidebar. */
const CATEGORY_ICONS: Record<string, ReactNode> = {
  repositories: <Database />,
  remotes: <CloudDownload />,
  distributions: <Send />,
  publications: <BookCheck />,
  content: <Boxes />,
  contentguards: <ShieldCheck />,
  acs: <Network />,
  exporters: <FileOutput />,
  importers: <FileInput />,
};

const OVERVIEW_ICONS: Record<string, ReactNode> = {
  dashboard: <House />,
  tasks: <ListChecks />,
};

function Sidebar({onNavigate}: {onNavigate?: () => void}) {
  const {registry, ctx, info} = useSession();
  const location = useLocation();
  const {simple, setMode} = useMode();
  const nav = useMemo(() => {
    const full = buildNav(registry, ctx);
    return simple ? simplifyNav(full) : full;
  }, [registry, ctx, simple]);
  const href = location.pathname.startsWith(`${base(ctx)}/`) ? hrefOfPath(location.pathname.slice(base(ctx).length), info.api_prefix) : null;
  const activePlugin =
    (href ? registry.typeForHref(href)?.plugin : undefined) ??
    nav.plugins.find((g) => g.items.some((i) => location.pathname.startsWith(i.to.split("?")[0])))?.id;
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set(activePlugin ? [activePlugin] : []));
  useEffect(() => {
    if (activePlugin) setExpanded((s) => (s.has(activePlugin) ? s : new Set([...s, activePlugin])));
  }, [activePlugin]);

  const adminActive =
    location.pathname === adminUrl(ctx) ||
    nav.admin.some((sec) => sec.items.some((i) => location.pathname === i.to || (href !== null && registry.typeForHref(href)?.id === i.id)));
  const [adminOpen, setAdminOpen] = useState(adminActive);
  useEffect(() => {
    if (adminActive) setAdminOpen(true);
  }, [adminActive]);

  const section = (title: string) => (
    <div className="px-3 pt-4 pb-1 text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">{title}</div>
  );
  return (
    <>
      <nav className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto p-3">
        {nav.overview.map((i) => (
          <NavEntry key={i.id} icon={OVERVIEW_ICONS[i.id]} item={i} onNavigate={onNavigate} />
        ))}
        {section("Plugins")}
        {nav.plugins.map((g) => (
          <Collapsible
            key={g.id}
            open={expanded.has(g.id)}
            onOpenChange={(open) =>
              setExpanded((s) => {
                const next = new Set(s);
                if (open) next.add(g.id);
                else next.delete(g.id);
                return next;
              })
            }
          >
            <CollapsibleTrigger className="group flex w-full items-center justify-between rounded-md px-3 py-1.5 text-sm font-medium hover:bg-sidebar-accent/60">
              {g.label}
              <ChevronRight className="size-4 text-muted-foreground transition-transform group-data-[state=open]:rotate-90" />
            </CollapsibleTrigger>
            <CollapsibleContent className="flex flex-col gap-0.5 ps-2 pb-1">
              {g.items.map((i) => (
                <NavEntry key={i.id} item={i} onNavigate={onNavigate} />
              ))}
            </CollapsibleContent>
          </Collapsible>
        ))}
        {nav.platform.length > 0 && (
          <>
            {section("All plugins")}
            {nav.platform.map((i) => (
              <NavEntry key={i.id} icon={CATEGORY_ICONS[i.category ?? ""] ?? <Boxes />} item={i} onNavigate={onNavigate} />
            ))}
          </>
        )}
        {simple && (
          <div className="mx-1 mt-6 rounded-lg border border-dashed p-3 text-xs leading-relaxed text-muted-foreground">
            Content, publications and administration (users, maintenance…) are in advanced mode.
            <Button className="mt-2 w-full" size="sm" variant="outline" onClick={() => setMode("advanced")}>
              Switch to advanced
            </Button>
          </div>
        )}
      </nav>
      {nav.admin.length > 0 && (
        <AdminNav active={adminActive} open={adminOpen} sections={nav.admin} onNavigate={onNavigate} onOpenChange={setAdminOpen} />
      )}
    </>
  );
}

/** Administration block, pinned under the sidebar's scrolling navigation. */
function AdminNav({
  sections,
  open,
  active,
  onOpenChange,
  onNavigate,
}: {
  sections: AdminSection[];
  open: boolean;
  active: boolean;
  onOpenChange: (open: boolean) => void;
  onNavigate?: () => void;
}) {
  const {ctx} = useSession();
  const location = useLocation();
  const onHome = location.pathname === adminUrl(ctx);
  const scanning = !!useStorageScan(ctx).state.data?.running;
  return (
    <Collapsible
      className={cn("shrink-0 border-t bg-muted/50 p-2", active && "bg-primary/5")}
      open={open}
      onOpenChange={onOpenChange}
    >
      <div className="flex items-center gap-1">
        <NavLink
          className={cn(
            "flex flex-1 items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm font-semibold transition-colors hover:bg-sidebar-accent/60 [&_svg]:size-4",
            onHome && "bg-sidebar-accent text-sidebar-accent-foreground",
          )}
          end
          to={adminUrl(ctx)}
          onClick={onNavigate}
        >
          <Settings className={cn(active ? "text-primary" : "text-muted-foreground")} />
          Administration
        </NavLink>
        <CollapsibleTrigger
          aria-label={open ? "Collapse administration" : "Expand administration"}
          className="group rounded-md p-1.5 text-muted-foreground hover:bg-sidebar-accent/60"
        >
          <ChevronRight className="size-4 transition-transform group-data-[state=open]:rotate-90" />
        </CollapsibleTrigger>
      </div>
      <CollapsibleContent className="flex max-h-[45vh] flex-col gap-0.5 overflow-y-auto">
        {sections.map((sec) => (
          <div key={sec.id} className="flex flex-col gap-0.5">
            <div className="px-2.5 pt-3 pb-1 text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">{sec.label}</div>
            {sec.items.map((i) => (
              <NavEntry
                key={i.id}
                icon={i.id === "storage" && scanning ? <Spinner /> : adminItemIcon(i.id)}
                item={i}
                title={i.id === "storage" && scanning ? "A scan is running" : ADMIN_DESCRIPTIONS[i.id]}
                onNavigate={onNavigate}
              />
            ))}
          </div>
        ))}
      </CollapsibleContent>
    </Collapsible>
  );
}

function ContextSwitcher() {
  const {ctx, info} = useSession();
  const contexts = useContexts();
  const navigate = useNavigate();
  const list = contexts.data?.contexts ?? [];
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button className="max-w-[260px]" size="sm" variant="outline">
          <span className={cn("size-2 shrink-0 rounded-full", info.readonly ? "bg-warning" : "bg-success")} />
          <span className="truncate font-medium">{ctx}</span>
          <ChevronDown className="opacity-60" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="min-w-64">
        <DropdownMenuLabel>Contexts</DropdownMenuLabel>
        {list.map((c) => (
          <DropdownMenuItem key={c.name} onSelect={() => navigate(base(c.name))}>
            <div className="flex min-w-0 flex-1 flex-col">
              <span className="flex items-center gap-2 font-medium">
                {c.name}
                {c.readonly && <Badge variant="warning">read-only</Badge>}
              </span>
              <span className="truncate text-xs text-muted-foreground">{c.base_url}</span>
            </div>
            {c.name === ctx && <Check className="ms-auto" />}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Progress of a task from its reports (`12 / 40 Downloading artifacts`), and the ratio when known. */
function taskProgress(t: Json): {text: string | null; ratio: number | null} {
  const reports = Array.isArray(t.progress_reports) ? (t.progress_reports as Json[]) : [];
  const r = reports.find((p) => p.state === "running" && typeof p.total === "number" && (p.total as number) > 0) ?? reports.find((p) => p.state === "running");
  if (!r) return {text: null, ratio: null};
  const done = Number(r.done ?? 0);
  const total = typeof r.total === "number" ? r.total : null;
  return {text: `${r.message ?? ""}${total ? ` ${done.toLocaleString()} / ${total.toLocaleString()}` : done ? ` ${done.toLocaleString()}` : ""}`.trim(), ratio: total ? done / total : null};
}

/**
 * Running and waiting tasks of the whole server (started here, from the CLI, by schedules or
 * other users), polled; tasks started from this page are listed first with their label.
 */
function RunningTasks() {
  const {tracked, ctx, pulp, info, refresh} = useSession();
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const active = useQuery({
    queryKey: ["pulp", ctx, "active-tasks"],
    queryFn: () =>
      pulp.list(`${info.api_prefix}tasks/`, {
        state__in: "running,waiting",
        ordering: "-pulp_created",
        limit: 10,
        fields: "pulp_href,name,state,started_at,pulp_created,progress_reports",
      }),
    refetchInterval: (q) => (q.state.data?.count || tracked.length ? Math.min(refresh, 3000) : refresh),
  });
  const server = active.data?.results ?? [];
  const labels = new Map(tracked.map((t) => [t.href, t.label]));
  const extra = tracked.filter((t) => !server.some((s) => s.pulp_href === t.href) && ["running", "waiting", "queued"].includes(t.state));
  const scan = useStorageScan(ctx).state.data;
  const scanning = !!scan?.running;
  const prevScanning = useRef(scanning);
  useEffect(() => {
    if (prevScanning.current && !scanning && scan) {
      const view = {label: "View", onClick: () => navigate(`${base(ctx)}/storage`)};
      if (scan.error) toast.error("Disk usage scan stopped", {description: scan.error, action: view});
      else toast.success("Disk usage computed", {action: view, duration: 8000});
    }
    prevScanning.current = scanning;
  }, [scanning, scan, ctx, navigate]);
  const count = Math.max(active.data?.count ?? 0, 0) + extra.length + (scanning ? 1 : 0);
  if (!count) return null;
  const running = server.filter((t) => t.state === "running").length + extra.length + (scanning ? 1 : 0);
  const ratio = scanRatio(scan?.progress);
  const rows = [
    ...extra.map((t) => ({href: t.href, label: t.label, state: t.state, progress: {text: t.progress ?? null, ratio: null}, since: null as string | null})),
    ...server.map((t) => ({
      href: t.pulp_href as string,
      label: labels.get(t.pulp_href as string) ?? taskName(t.name),
      state: t.state as string,
      progress: taskProgress(t),
      since: (t.started_at ?? t.pulp_created) as string | null,
    })),
  ];
  const mine = (h: string) => labels.has(h);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button size="sm" variant="outline">
          {running > 0 ? <Spinner /> : <Clock />}
          {running > 0 ? `${running} running` : ""}
          {count - running > 0 && <span className="text-muted-foreground">{running > 0 ? " · " : ""}{count - running} waiting</span>}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="flex w-96 flex-col gap-1 p-2">
        <p className="px-2 pt-1 pb-2 text-sm font-semibold">In progress</p>
        {scanning && scan?.progress && (
          <button
            className="flex flex-col gap-1.5 rounded-md p-2 text-left hover:bg-muted/60"
            onClick={() => {
              setOpen(false);
              navigate(`${base(ctx)}/storage`);
            }}
          >
            <span className="flex items-center gap-2 text-sm font-medium">
              <HardDrive className="size-4 text-muted-foreground" />
              <span className="truncate">Disk usage scan</span>
              <Badge variant="secondary">pulply</Badge>
              <span className="ms-auto shrink-0 text-xs font-normal text-muted-foreground tabular-nums">
                {ratio !== null ? `${Math.round(ratio * 100)}%` : ""}
              </span>
            </span>
            <span className="truncate text-xs text-muted-foreground">
              Step {scan.progress.step || 1}/{scan.progress.steps || 5} · {scan.progress.label || "Starting"}
              {scan.progress.total > 0 && ` · ${scan.progress.done.toLocaleString()} / ${scan.progress.total.toLocaleString()}`}
            </span>
            <Progress aria-label="Disk usage scan" value={ratio !== null ? ratio * 100 : null} />
          </button>
        )}
        {rows.map((t) => (
          <button
            key={t.href}
            className="flex flex-col gap-1.5 rounded-md p-2 text-left hover:bg-muted/60"
            onClick={() => {
              setOpen(false);
              navigate(itemUrl(ctx, t.href));
            }}
          >
            <span className="flex items-center gap-2 text-sm font-medium">
              <span className="truncate">{t.label}</span>
              {mine(t.href) && <Badge variant="secondary">started here</Badge>}
              <span className="ms-auto shrink-0 text-xs font-normal text-muted-foreground">
                {t.state === "waiting" ? "waiting" : t.since ? relativeTime(t.since) : t.state}
              </span>
            </span>
            {t.progress.text && <span className="truncate text-xs text-muted-foreground">{t.progress.text}</span>}
            {t.state !== "waiting" && <Progress aria-label={t.label} value={t.progress.ratio !== null ? t.progress.ratio * 100 : null} />}
          </button>
        ))}
        {(active.data?.count ?? 0) > server.length && (
          <button
            className="px-2 py-1.5 text-left text-xs text-primary hover:underline"
            onClick={() => {
              setOpen(false);
              navigate(listUrl(ctx, "tasks", {state__in: "running,waiting"}));
            }}
          >
            All {active.data?.count.toLocaleString()} tasks in progress →
          </button>
        )}
      </PopoverContent>
    </Popover>
  );
}

const MODE_ITEM =
  "px-3 text-muted-foreground data-[state=on]:bg-primary data-[state=on]:text-primary-foreground data-[state=on]:hover:bg-primary";

function ModeSwitch() {
  const {mode, setMode} = useMode();
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <ToggleGroup
          aria-label="Interface mode"
          className="rounded-lg border bg-muted/40 p-0.5"
          size="sm"
          type="single"
          value={mode}
          onValueChange={(v) => v && setMode(v as "simple" | "advanced")}
        >
          <ToggleGroupItem className={MODE_ITEM} value="simple">
            Simple
          </ToggleGroupItem>
          <ToggleGroupItem className={MODE_ITEM} value="advanced">
            Advanced
          </ToggleGroupItem>
        </ToggleGroup>
      </TooltipTrigger>
      <TooltipContent>
        {mode === "simple" ? "Everyday resources and actions" : "Every resource type, field and operation"}
      </TooltipContent>
    </Tooltip>
  );
}

function ThemeButton() {
  const {theme, setTheme} = useTheme();
  const next = theme === "light" ? "dark" : theme === "dark" ? "system" : "light";
  const icon = theme === "light" ? <Sun /> : theme === "dark" ? <Moon /> : <Monitor />;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button aria-label={`Theme: ${theme}`} size="icon-sm" variant="ghost" onClick={() => setTheme(next)}>
          {icon}
        </Button>
      </TooltipTrigger>
      <TooltipContent>Theme: {theme}</TooltipContent>
    </Tooltip>
  );
}

function AccountMenu() {
  const {info, ctx, reloadRegistry} = useSession();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const initials = (info.username ?? "?").slice(0, 2).toUpperCase();
  const reload = async () => {
    const id = toast.loading("Reloading the API schema…");
    try {
      await reloadRegistry();
      toast.success("API schema reloaded", {id, duration: 3000});
    } catch (e) {
      toast.error("Cannot reload the schema", {id, description: errorText(e)});
    }
  };
  const disconnect = async () => {
    await postJson(`/api/contexts/${encodeURIComponent(ctx)}/session`, {}, "DELETE").catch(() => undefined);
    queryClient.removeQueries({queryKey: ["session", ctx]});
    navigate("/");
  };
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button aria-label="Account" className="rounded-full" size="icon-sm" variant="ghost">
          <Avatar className="size-7">
            <AvatarFallback className="bg-primary/10 text-xs text-primary">{initials}</AvatarFallback>
          </Avatar>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-56">
        <DropdownMenuGroup>
          <DropdownMenuItem onSelect={() => navigate(`${base(ctx)}/me`)}>
            <User />
            <div className="flex flex-col">
              <span className="font-medium">{info.username ?? "anonymous"}</span>
              <span className="text-xs text-muted-foreground">My account & permissions</span>
            </div>
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => void reload()}>
            <RefreshCw />
            Reload API schema
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem variant="destructive" onSelect={() => void disconnect()}>
          Disconnect
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Icon of a destination of the palette. */
function destinationIcon(d: NavItem): ReactNode {
  if (OVERVIEW_ICONS[d.id]) return OVERVIEW_ICONS[d.id];
  if (d.id === "admin") return <Settings />;
  if (d.id === "me") return <User />;
  if (d.hint?.startsWith("Administration")) return adminItemIcon(d.id);
  return CATEGORY_ICONS[d.category ?? ""] ?? <Boxes />;
}

const SEARCHED = ["repositories", "distributions", "remotes"];

/** Resources of the server whose name contains the query (repositories, distributions, remotes). */
function useResourceSearch(term: string) {
  const {registry, pulp, ctx} = useSession();
  const [debounced, setDebounced] = useState(term);
  useEffect(() => {
    const t = setTimeout(() => setDebounced(term), 200);
    return () => clearTimeout(t);
  }, [term]);
  const types = SEARCHED.map((id) => registry.get(id)).filter((t): t is NonNullable<typeof t> => !!t?.list_path);
  return useQuery({
    queryKey: ["pulp", ctx, "palette", debounced],
    enabled: debounced.length >= 2,
    staleTime: 10_000,
    queryFn: async () => {
      const lists = await Promise.all(
        types.map((t) =>
          pulp
            .list(t.list_path!, {name__icontains: debounced, limit: 5, fields: "pulp_href,name,base_path,url"})
            .then((p) => p.results.map((item) => ({item, category: t.id})))
            .catch(() => []),
        ),
      );
      return lists.flat();
    },
  });
}

const PALETTE_ITEM =
  "flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2.5 text-sm outline-none select-none data-[selected=true]:bg-accent data-[selected=true]:text-accent-foreground [&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground";

function CommandPalette({isOpen, onOpenChange}: {isOpen: boolean; onOpenChange: (o: boolean) => void}) {
  const {registry, ctx} = useSession();
  const navigate = useNavigate();
  const [q, setQ] = useState("");
  const all = useMemo(() => allDestinations(registry, ctx), [registry, ctx]);
  const term = q.trim();
  const pages = useMemo(() => {
    const words = term.toLowerCase().split(/\s+/).filter(Boolean);
    if (!words.length) return all.filter((d) => ["dashboard", "tasks", "admin", "storage", "operations", "me"].includes(d.id) || d.hint === "All plugins");
    return all.filter((d) => words.every((w) => `${d.label} ${d.hint ?? ""} ${d.id}`.toLowerCase().includes(w))).slice(0, 30);
  }, [all, term]);
  const found = useResourceSearch(term);
  const resources = found.data ?? [];
  const close = () => {
    onOpenChange(false);
    setQ("");
  };
  const go = (to: string) => {
    navigate(to);
    close();
  };
  const isMac = navigator.platform.toLowerCase().includes("mac");
  return (
    <Dialog open={isOpen} onOpenChange={(o) => (o ? onOpenChange(true) : close())}>
      <DialogContent
        aria-describedby={undefined}
        className="top-[12vh] flex max-h-[76vh] translate-y-0 flex-col gap-0 overflow-hidden rounded-2xl p-0 sm:max-w-2xl"
        showCloseButton={false}
      >
        <DialogTitle className="sr-only">Search</DialogTitle>
        <CommandPrimitive className="flex min-h-0 flex-1 flex-col" shouldFilter={false}>
          <div className="flex items-center gap-3 border-b px-4">
            <Search className="size-5 shrink-0 text-muted-foreground" />
            <CommandPrimitive.Input
              autoFocus
              className="h-14 min-w-0 flex-1 bg-transparent text-base outline-none placeholder:text-muted-foreground"
              placeholder="Search pages, repositories, distributions, remotes…"
              value={q}
              onValueChange={setQ}
            />
            {found.isFetching && <Spinner />}
            <Kbd>Esc</Kbd>
          </div>
          <CommandPrimitive.List className="min-h-0 flex-1 overflow-y-auto p-2">
            <CommandPrimitive.Empty className="py-10 text-center text-sm text-muted-foreground">
              {found.isFetching ? "Searching…" : `Nothing matches “${term}”`}
            </CommandPrimitive.Empty>
            {resources.length > 0 && (
              <CommandPrimitive.Group
                className="pb-2 **:[[cmdk-group-heading]]:px-3 **:[[cmdk-group-heading]]:py-1.5 **:[[cmdk-group-heading]]:text-xs **:[[cmdk-group-heading]]:font-medium **:[[cmdk-group-heading]]:text-muted-foreground"
                heading="On the server"
              >
                {resources.map(({item, category}) => {
                  const href = item.pulp_href as string;
                  const t = registry.typeForHref(href);
                  return (
                    <CommandPrimitive.Item key={href} className={PALETTE_ITEM} value={`r:${href}`} onSelect={() => go(itemUrl(ctx, href))}>
                      {CATEGORY_ICONS[category]}
                      <span className="min-w-0 flex-1 truncate font-medium">{String(item.name ?? href)}</span>
                      <span className="shrink-0 text-xs text-muted-foreground">{t ? typeTitle(t) : categoryLabel(category)}</span>
                    </CommandPrimitive.Item>
                  );
                })}
              </CommandPrimitive.Group>
            )}
            {pages.length > 0 && (
              <CommandPrimitive.Group
                className="**:[[cmdk-group-heading]]:px-3 **:[[cmdk-group-heading]]:py-1.5 **:[[cmdk-group-heading]]:text-xs **:[[cmdk-group-heading]]:font-medium **:[[cmdk-group-heading]]:text-muted-foreground"
                heading={term ? "Pages" : "Go to"}
              >
                {pages.map((d) => (
                  <CommandPrimitive.Item key={d.to} className={PALETTE_ITEM} value={`p:${d.to}`} onSelect={() => go(d.to)}>
                    {destinationIcon(d)}
                    <span className="min-w-0 flex-1 truncate">{d.label}</span>
                    {d.hint && <span className="shrink-0 text-xs text-muted-foreground">{d.hint}</span>}
                  </CommandPrimitive.Item>
                ))}
              </CommandPrimitive.Group>
            )}
          </CommandPrimitive.List>
          <div className="flex items-center gap-4 border-t px-4 py-2 text-xs text-muted-foreground">
            <span className="flex items-center gap-1">
              <Kbd>↑</Kbd>
              <Kbd>↓</Kbd> move
            </span>
            <span className="flex items-center gap-1">
              <Kbd>↵</Kbd> open
            </span>
            <span className="ms-auto flex items-center gap-1">
              <KbdGroup>
                <Kbd>{isMac ? "⌘" : "Ctrl"}</Kbd>
                <Kbd>K</Kbd>
              </KbdGroup>
              anywhere
            </span>
          </div>
        </CommandPrimitive>
      </DialogContent>
    </Dialog>
  );
}

export function Shell({children}: {children: ReactNode}) {
  const {info, ctx} = useSession();
  const {simple} = useMode();
  const [menu, setMenu] = useState(false);
  const [palette, setPalette] = useState(false);
  const navigate = useNavigate();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const isMac = navigator.platform.toLowerCase().includes("mac");

  return (
    <div className="flex h-full">
      <aside className="hidden w-64 shrink-0 flex-col border-r bg-sidebar text-sidebar-foreground lg:flex">
        <button
          aria-label="Dashboard"
          className="group mx-2 mt-2 cursor-pointer rounded-lg px-3 pt-2 pb-1 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
          title="Dashboard"
          onClick={() => navigate(base(ctx))}
        >
          <Logo interactive />
        </button>
        <div className="flex min-h-0 flex-1 flex-col">
          <Sidebar />
        </div>
      </aside>
      <Sheet open={menu} onOpenChange={setMenu}>
        <SheetContent className="w-72 gap-0 bg-sidebar p-0 text-sidebar-foreground" side="left">
          <SheetHeader className="px-5 pt-4 pb-0">
            <SheetTitle>
              <button
                aria-label="Dashboard"
                className="group cursor-pointer rounded-lg text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
                onClick={() => {
                  setMenu(false);
                  navigate(base(ctx));
                }}
              >
                <Logo interactive />
              </button>
            </SheetTitle>
          </SheetHeader>
          <div className="flex min-h-0 flex-1 flex-col">
            <Sidebar onNavigate={() => setMenu(false)} />
          </div>
        </SheetContent>
      </Sheet>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex items-center gap-2 border-b px-4 py-2.5">
          <Button aria-label="Menu" className="lg:hidden" size="icon-sm" variant="ghost" onClick={() => setMenu(true)}>
            <Menu />
          </Button>
          <ContextSwitcher />
          {info.readonly && <Badge variant="warning">Read-only</Badge>}
          {info.domain && <Badge variant="secondary">domain {info.domain}</Badge>}
          <button
            className="ms-2 hidden h-9 max-w-2xl min-w-0 flex-1 items-center gap-2 rounded-lg border bg-muted/40 px-3 text-sm text-muted-foreground transition-colors hover:bg-muted md:flex"
            onClick={() => setPalette(true)}
          >
            <Search className="size-4" />
            <span className="flex-1 text-left">Search repositories, remotes, tasks…</span>
            <KbdGroup>
              <Kbd>{isMac ? "⌘" : "Ctrl"}</Kbd>
              <Kbd>K</Kbd>
            </KbdGroup>
          </button>
          <div className="ms-auto flex items-center gap-1.5">
            <Button aria-label="Go to" className="md:hidden" size="icon-sm" variant="ghost" onClick={() => setPalette(true)}>
              <Search />
            </Button>
            <RunningTasks />
            <ModeSwitch />
            {!simple && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button aria-label="Administration" size="icon-sm" variant="ghost" onClick={() => navigate(adminUrl(ctx))}>
                    <Settings />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Administration</TooltipContent>
              </Tooltip>
            )}
            <ThemeButton />
            <AccountMenu />
          </div>
        </header>
        <main className="flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-[1800px] p-4 sm:p-6 xl:px-10">{children}</div>
        </main>
      </div>
      <CommandPalette isOpen={palette} onOpenChange={setPalette} />
    </div>
  );
}
