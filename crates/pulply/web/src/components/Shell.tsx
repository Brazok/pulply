// Application frame: sidebar navigation, top bar (context, search, running tasks,
// theme, account) and the command palette (Ctrl/⌘ K).

import {useQueryClient} from "@tanstack/react-query";
import {
  Check,
  ChevronDown,
  ChevronRight,
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
  Wrench,
} from "lucide-react";
import {type ReactNode, useEffect, useMemo, useState} from "react";
import {NavLink, useLocation, useNavigate} from "react-router";
import {toast} from "sonner";

import {postJson} from "../lib/api";
import {taskName} from "../lib/format";
import {type NavItem, allDestinations, buildNav, simplifyNav} from "../lib/nav";
import {base, itemUrl} from "../lib/routes";
import {errorText, useContexts, useSession} from "../lib/session";
import {useMode} from "../lib/mode";
import {useTheme} from "../lib/theme";
import {cn} from "../lib/utils";
import {Logo} from "./screens";
import {Avatar, AvatarFallback} from "./ui/avatar";
import {Badge} from "./ui/badge";
import {Button} from "./ui/button";
import {Collapsible, CollapsibleContent, CollapsibleTrigger} from "./ui/collapsible";
import {Command, CommandDialog, CommandEmpty, CommandInput, CommandItem, CommandList, CommandShortcut} from "./ui/command";
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

function NavEntry({item, icon, onNavigate}: {item: NavItem; icon?: ReactNode; onNavigate?: () => void}) {
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
      to={item.to}
      onClick={onNavigate}
    >
      {icon}
      <span className="truncate">{item.label}</span>
    </NavLink>
  );
}

const OVERVIEW_ICONS: Record<string, ReactNode> = {
  dashboard: <House />,
  tasks: <ListChecks />,
  operations: <Wrench />,
};

function Sidebar({onNavigate}: {onNavigate?: () => void}) {
  const {registry, ctx} = useSession();
  const location = useLocation();
  const {simple, setMode} = useMode();
  const nav = useMemo(() => {
    const full = buildNav(registry, ctx);
    return simple ? simplifyNav(full) : full;
  }, [registry, ctx, simple]);
  const href = new URLSearchParams(location.search).get("href");
  const activePlugin =
    (href ? registry.typeForHref(href)?.plugin : undefined) ??
    nav.plugins.find((g) => g.items.some((i) => location.pathname.startsWith(i.to.split("?")[0])))?.id;
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set(activePlugin ? [activePlugin] : []));
  useEffect(() => {
    if (activePlugin) setExpanded((s) => (s.has(activePlugin) ? s : new Set([...s, activePlugin])));
  }, [activePlugin]);

  const section = (title: string) => (
    <div className="px-3 pt-4 pb-1 text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">{title}</div>
  );
  return (
    <nav className="flex flex-col gap-0.5 p-3">
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
      {nav.platform.length > 0 && section("All plugins")}
      {simple && (
        <div className="mx-1 mt-6 rounded-lg border border-dashed p-3 text-xs leading-relaxed text-muted-foreground">
          Content, publications, users and maintenance are in advanced mode.
          <Button className="mt-2 w-full" size="sm" variant="outline" onClick={() => setMode("advanced")}>
            Switch to advanced
          </Button>
        </div>
      )}
      {nav.platform.map((i) => (
        <NavEntry key={i.id} item={i} onNavigate={onNavigate} />
      ))}
      {nav.admin.length > 0 && section("Administration")}
      {nav.admin.map((i) => (
        <NavEntry key={i.id} item={i} onNavigate={onNavigate} />
      ))}
    </nav>
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

function RunningTasks() {
  const {tracked, ctx} = useSession();
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  if (!tracked.length) return null;
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button size="sm" variant="outline">
          <Spinner />
          {tracked.length} running
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="flex w-80 flex-col gap-3">
        <p className="text-sm font-semibold">Tasks started here</p>
        {tracked.map((t) => (
          <button
            key={t.href}
            className="flex flex-col gap-1.5 rounded-md p-1 text-left hover:bg-muted/60"
            onClick={() => {
              setOpen(false);
              navigate(itemUrl(ctx, t.href));
            }}
          >
            <span className="text-sm font-medium">{t.label}</span>
            <span className="text-xs text-muted-foreground">{t.progress ?? taskName(t.state)}</span>
            <Progress aria-label={t.label} value={null} />
          </button>
        ))}
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

function CommandPalette({isOpen, onOpenChange}: {isOpen: boolean; onOpenChange: (o: boolean) => void}) {
  const {registry, ctx} = useSession();
  const navigate = useNavigate();
  const [q, setQ] = useState("");
  const all = useMemo(() => allDestinations(registry, ctx), [registry, ctx]);
  const results = useMemo(() => {
    const words = q.toLowerCase().split(/\s+/).filter(Boolean);
    return all.filter((d) => words.every((w) => `${d.label} ${d.hint ?? ""} ${d.id}`.toLowerCase().includes(w))).slice(0, 12);
  }, [all, q]);
  const go = (d: NavItem) => {
    navigate(d.to);
    onOpenChange(false);
    setQ("");
  };
  return (
    <CommandDialog description="Jump to any page of the server" open={isOpen} title="Go to" onOpenChange={onOpenChange}>
      <Command shouldFilter={false}>
        <CommandInput placeholder="Go to… (repositories rpm, tasks, users…)" value={q} onValueChange={setQ} />
        <CommandList>
          <CommandEmpty>No match</CommandEmpty>
          {results.map((d) => (
            <CommandItem key={d.to} value={d.to} onSelect={() => go(d)}>
              <span>{d.label}</span>
              {d.hint && <CommandShortcut className="tracking-normal">{d.hint}</CommandShortcut>}
            </CommandItem>
          ))}
        </CommandList>
      </Command>
    </CommandDialog>
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
        <button className="px-5 pt-4 pb-1 text-left" onClick={() => navigate(base(ctx))}>
          <Logo />
        </button>
        <div className="flex-1 overflow-y-auto">
          <Sidebar />
        </div>
      </aside>
      <Sheet open={menu} onOpenChange={setMenu}>
        <SheetContent className="w-72 gap-0 bg-sidebar p-0 text-sidebar-foreground" side="left">
          <SheetHeader className="px-5 pt-4 pb-0">
            <SheetTitle>
              <Logo />
            </SheetTitle>
          </SheetHeader>
          <div className="flex-1 overflow-y-auto">
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
                  <Button aria-label="Maintenance" size="icon-sm" variant="ghost" onClick={() => navigate(`${base(ctx)}/operations`)}>
                    <Settings />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Maintenance</TooltipContent>
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
