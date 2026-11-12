// Application frame: sidebar navigation, top bar (context, search, running tasks,
// theme, account) and the command palette (Ctrl/⌘ K).

import {
  ArrowsRotateRight,
  Bars,
  ChevronDown,
  Display,
  Gear,
  House,
  ListCheck,
  Magnifier,
  Moon,
  Person,
  Sun,
  Wrench,
} from "@gravity-ui/icons";
import {
  Accordion,
  Avatar,
  Button,
  Chip,
  Drawer,
  Dropdown,
  Kbd,
  Label,
  Modal,
  Popover,
  ProgressBar,
  SearchField,
  Separator,
  Spinner,
  toast,
  useTheme,
} from "@heroui/react";
import {useQueryClient} from "@tanstack/react-query";
import {type ReactNode, useEffect, useMemo, useState} from "react";
import {NavLink, useLocation, useNavigate} from "react-router";

import {postJson} from "../lib/api";
import {taskName} from "../lib/format";
import {type NavItem, allDestinations, buildNav} from "../lib/nav";
import {base, itemUrl} from "../lib/routes";
import {errorText, useContexts, useSession} from "../lib/session";
import {Logo} from "./screens";

function NavEntry({item, icon, onNavigate}: {item: NavItem; icon?: ReactNode; onNavigate?: () => void}) {
  const location = useLocation();
  const exact = item.id === "dashboard";
  return (
    <NavLink
      className={({isActive}) => {
        const active = exact ? location.pathname === item.to || location.pathname === `${item.to}/` : isActive;
        return `flex items-center gap-2.5 rounded-xl px-3 py-1.5 text-sm transition-colors ${
          active ? "bg-accent-soft text-accent-soft-foreground font-medium" : "text-foreground/80 hover:bg-default/60"
        }`;
      }}
      end={exact}
      to={item.to}
      onClick={onNavigate}
    >
      {icon}
      <span className="truncate">{item.label}</span>
    </NavLink>
  );
}

const OVERVIEW_ICONS: Record<string, ReactNode> = {
  dashboard: <House className="size-4" />,
  tasks: <ListCheck className="size-4" />,
  operations: <Wrench className="size-4" />,
};

function Sidebar({onNavigate}: {onNavigate?: () => void}) {
  const {registry, ctx} = useSession();
  const location = useLocation();
  const nav = useMemo(() => buildNav(registry, ctx), [registry, ctx]);
  const href = new URLSearchParams(location.search).get("href");
  const activePlugin =
    (href ? registry.typeForHref(href)?.plugin : undefined) ??
    nav.plugins.find((g) => g.items.some((i) => location.pathname.startsWith(i.to.split("?")[0])))?.id;
  const [expanded, setExpanded] = useState<Set<string | number>>(() => new Set(activePlugin ? [activePlugin] : []));
  useEffect(() => {
    if (activePlugin) setExpanded((s) => (s.has(activePlugin) ? s : new Set([...s, activePlugin])));
  }, [activePlugin]);

  const section = (title: string) => (
    <div className="px-3 pb-1 pt-4 text-[11px] font-semibold uppercase tracking-wider text-muted">{title}</div>
  );
  return (
    <nav className="flex flex-col gap-0.5 p-3">
      {nav.overview.map((i) => (
        <NavEntry key={i.id} icon={OVERVIEW_ICONS[i.id]} item={i} onNavigate={onNavigate} />
      ))}
      {section("Plugins")}
      <Accordion
        allowsMultipleExpanded
        hideSeparator
        expandedKeys={expanded}
        onExpandedChange={(keys) => setExpanded(new Set(keys))}
      >
        {nav.plugins.map((g) => (
          <Accordion.Item key={g.id} id={g.id}>
            <Accordion.Heading>
              <Accordion.Trigger className="rounded-xl px-3 py-1.5 text-sm font-medium hover:bg-default/60">
                {g.label}
                <Accordion.Indicator>
                  <ChevronDown />
                </Accordion.Indicator>
              </Accordion.Trigger>
            </Accordion.Heading>
            <Accordion.Panel>
              <Accordion.Body className="flex flex-col gap-0.5 ps-2 pb-1">
                {g.items.map((i) => (
                  <NavEntry key={i.id} item={i} onNavigate={onNavigate} />
                ))}
              </Accordion.Body>
            </Accordion.Panel>
          </Accordion.Item>
        ))}
      </Accordion>
      {nav.platform.length > 0 && section("All plugins")}
      {nav.platform.map((i) => (
        <NavEntry key={i.id} item={i} onNavigate={onNavigate} />
      ))}
      {section("Administration")}
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
    <Dropdown>
      <Button className="max-w-[260px]" size="sm" variant="secondary">
        <span className={`size-2 shrink-0 rounded-full ${info.readonly ? "bg-warning" : "bg-success"}`} />
        <span className="truncate font-medium">{ctx}</span>
        <ChevronDown className="size-3 shrink-0" />
      </Button>
      <Dropdown.Popover placement="bottom start">
        <Dropdown.Menu
          selectedKeys={new Set([ctx])}
          selectionMode="single"
          onAction={(key) => navigate(base(String(key)))}
        >
          {list.map((c) => (
            <Dropdown.Item key={c.name} id={c.name} textValue={c.name}>
              <div className="flex flex-col">
                <Label className="flex items-center gap-2">
                  {c.name}
                  {c.readonly && (
                    <Chip color="warning" size="sm" variant="soft">
                      read-only
                    </Chip>
                  )}
                </Label>
                <span className="text-xs text-muted">{c.base_url}</span>
              </div>
              <Dropdown.ItemIndicator />
            </Dropdown.Item>
          ))}
        </Dropdown.Menu>
      </Dropdown.Popover>
    </Dropdown>
  );
}

function RunningTasks() {
  const {tracked, ctx} = useSession();
  const navigate = useNavigate();
  if (!tracked.length) return null;
  return (
    <Popover>
      <Button size="sm" variant="secondary">
        <Spinner color="current" size="sm" />
        {tracked.length} running
      </Button>
      <Popover.Content className="w-80" placement="bottom end">
        <Popover.Dialog className="flex flex-col gap-3">
          <Popover.Heading className="text-sm font-semibold">Tasks started here</Popover.Heading>
          {tracked.map((t) => (
            <button key={t.href} className="flex flex-col gap-1 text-left" onClick={() => navigate(itemUrl(ctx, t.href))}>
              <span className="text-sm font-medium">{t.label}</span>
              <span className="text-xs text-muted">{t.progress ?? taskName(t.state)}</span>
              <ProgressBar isIndeterminate aria-label={t.label} size="sm">
                <ProgressBar.Track>
                  <ProgressBar.Fill />
                </ProgressBar.Track>
              </ProgressBar>
            </button>
          ))}
        </Popover.Dialog>
      </Popover.Content>
    </Popover>
  );
}

function ThemeButton() {
  const {theme, setTheme} = useTheme("system");
  const next = theme === "light" ? "dark" : theme === "dark" ? "system" : "light";
  const icon = theme === "light" ? <Sun /> : theme === "dark" ? <Moon /> : <Display />;
  return (
    <Button isIconOnly aria-label={`Theme: ${theme}`} size="sm" variant="ghost" onPress={() => setTheme(next)}>
      {icon}
    </Button>
  );
}

function AccountMenu() {
  const {info, ctx, reloadRegistry} = useSession();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const initials = (info.username ?? "?").slice(0, 2).toUpperCase();
  return (
    <Dropdown>
      <Button isIconOnly aria-label="Account" className="rounded-full" size="sm" variant="ghost">
        <Avatar size="sm" variant="soft">
          <Avatar.Fallback>{initials}</Avatar.Fallback>
        </Avatar>
      </Button>
      <Dropdown.Popover placement="bottom end">
        <Dropdown.Menu
          onAction={async (key) => {
            if (key === "me") navigate(`${base(ctx)}/me`);
            if (key === "schema") {
              const id = toast("Reloading the API schema…", {isLoading: true, timeout: 0});
              try {
                await reloadRegistry();
                toast.update(id, "API schema reloaded", {variant: "success", timeout: 3000});
              } catch (e) {
                toast.update(id, "Cannot reload the schema", {description: errorText(e), variant: "danger"});
              }
            }
            if (key === "disconnect") {
              await postJson(`/api/contexts/${encodeURIComponent(ctx)}/session`, {}, "DELETE").catch(() => undefined);
              queryClient.removeQueries({queryKey: ["session", ctx]});
              navigate("/");
            }
          }}
        >
          <Dropdown.Section>
            <Dropdown.Item id="me" textValue="My account">
              <Person className="size-4 text-muted" />
              <div className="flex flex-col">
                <Label>{info.username ?? "anonymous"}</Label>
                <span className="text-xs text-muted">My account & permissions</span>
              </div>
            </Dropdown.Item>
            <Dropdown.Item id="schema" textValue="Reload API schema">
              <ArrowsRotateRight className="size-4 text-muted" />
              <Label>Reload API schema</Label>
            </Dropdown.Item>
          </Dropdown.Section>
          <Separator />
          <Dropdown.Item id="disconnect" textValue="Disconnect" variant="danger">
            <Label>Disconnect</Label>
          </Dropdown.Item>
        </Dropdown.Menu>
      </Dropdown.Popover>
    </Dropdown>
  );
}

function CommandPalette({isOpen, onOpenChange}: {isOpen: boolean; onOpenChange: (o: boolean) => void}) {
  const {registry, ctx} = useSession();
  const navigate = useNavigate();
  const [q, setQ] = useState("");
  const [index, setIndex] = useState(0);
  const all = useMemo(() => allDestinations(registry, ctx), [registry, ctx]);
  const results = useMemo(() => {
    const words = q.toLowerCase().split(/\s+/).filter(Boolean);
    return all.filter((d) => words.every((w) => `${d.label} ${d.hint ?? ""} ${d.id}`.toLowerCase().includes(w))).slice(0, 12);
  }, [all, q]);
  useEffect(() => setIndex(0), [q]);
  const go = (d: NavItem | undefined) => {
    if (!d) return;
    navigate(d.to);
    onOpenChange(false);
    setQ("");
  };
  return (
    <Modal.Backdrop isOpen={isOpen} variant="blur" onOpenChange={onOpenChange}>
      <Modal.Container placement="top" size="md">
        <Modal.Dialog aria-label="Go to">
          <Modal.Body className="flex flex-col gap-2 p-1">
            <SearchField
              autoFocus
              fullWidth
              aria-label="Go to"
              value={q}
              onChange={setQ}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") setIndex((i) => Math.min(i + 1, results.length - 1));
                else if (e.key === "ArrowUp") setIndex((i) => Math.max(i - 1, 0));
                else return;
                e.preventDefault();
              }}
              onSubmit={() => go(results[index])}
            >
              <SearchField.Group>
                <SearchField.SearchIcon />
                <SearchField.Input placeholder="Go to… (repositories rpm, tasks, users…)" />
              </SearchField.Group>
            </SearchField>
            <ul className="flex flex-col">
              {results.map((d, i) => (
                <li key={d.to}>
                  <button
                    className={`flex w-full items-center justify-between rounded-xl px-3 py-2 text-left text-sm ${
                      i === index ? "bg-accent-soft text-accent-soft-foreground" : "hover:bg-default/60"
                    }`}
                    onClick={() => go(d)}
                    onMouseEnter={() => setIndex(i)}
                  >
                    <span>{d.label}</span>
                    {d.hint && <span className="text-xs text-muted">{d.hint}</span>}
                  </button>
                </li>
              ))}
              {!results.length && <li className="px-3 py-6 text-center text-sm text-muted">No match</li>}
            </ul>
          </Modal.Body>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}

export function Shell({children}: {children: ReactNode}) {
  const {info, ctx} = useSession();
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
      <aside className="hidden w-64 shrink-0 flex-col border-r border-separator bg-surface lg:flex">
        <button className="px-5 pt-4 pb-1 text-left" onClick={() => navigate(base(ctx))}>
          <Logo />
        </button>
        <div className="flex-1 overflow-y-auto">
          <Sidebar />
        </div>
      </aside>
      <Drawer.Backdrop isOpen={menu} onOpenChange={setMenu}>
        <Drawer.Content placement="left">
          <Drawer.Dialog className="w-72 p-0">
            <Drawer.Header className="px-5 pt-4">
              <Drawer.Heading>
                <Logo />
              </Drawer.Heading>
            </Drawer.Header>
            <Drawer.Body className="p-0">
              <Sidebar onNavigate={() => setMenu(false)} />
            </Drawer.Body>
          </Drawer.Dialog>
        </Drawer.Content>
      </Drawer.Backdrop>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex items-center gap-2 border-b border-separator px-4 py-2.5">
          <Button isIconOnly aria-label="Menu" className="lg:hidden" size="sm" variant="ghost" onPress={() => setMenu(true)}>
            <Bars />
          </Button>
          <ContextSwitcher />
          {info.readonly && (
            <Chip color="warning" size="sm" variant="soft">
              Read-only
            </Chip>
          )}
          {info.domain && (
            <Chip size="sm" variant="secondary">
              domain {info.domain}
            </Chip>
          )}
          <button
            className="ms-2 hidden h-8 min-w-0 flex-1 max-w-sm items-center gap-2 rounded-xl bg-default/60 px-3 text-sm text-muted hover:bg-default md:flex"
            onClick={() => setPalette(true)}
          >
            <Magnifier className="size-4" />
            <span className="flex-1 text-left">Go to…</span>
            <Kbd variant="light">
              <Kbd.Abbr keyValue={isMac ? "command" : "ctrl"} />
              <Kbd.Content>K</Kbd.Content>
            </Kbd>
          </button>
          <div className="ms-auto flex items-center gap-1.5">
            <Button isIconOnly aria-label="Go to" className="md:hidden" size="sm" variant="ghost" onPress={() => setPalette(true)}>
              <Magnifier />
            </Button>
            <RunningTasks />
            <Button
              isIconOnly
              aria-label="Maintenance"
              size="sm"
              variant="ghost"
              onPress={() => navigate(`${base(ctx)}/operations`)}
            >
              <Gear />
            </Button>
            <ThemeButton />
            <AccountMenu />
          </div>
        </header>
        <main className="flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-[1400px] p-4 sm:p-6">{children}</div>
        </main>
      </div>
      <CommandPalette isOpen={palette} onOpenChange={setPalette} />
    </div>
  );
}

