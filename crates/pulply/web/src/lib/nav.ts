// Navigation tree built from the registry: overview, one group per plugin, platform
// (cross-plugin and core content management) and administration.

import {categoryLabel, pluginLabel, typeLabel} from "./format";
import {PLUGIN_CATEGORIES, type Registry, type ResourceType} from "./registry";
import {adminUrl, base, listUrl} from "./routes";

export interface NavItem {
  id: string;
  label: string;
  to: string;
  /** Secondary text for search results. */
  hint?: string;
  /** Category of the listed type (plugin groups). */
  category?: string;
  /** Variant of a type (`Remotes · Git`), as opposed to the plugin's main one. */
  variant?: boolean;
}

export interface NavGroup {
  id: string;
  label: string;
  items: NavItem[];
}

const CATEGORY_ORDER = [...PLUGIN_CATEGORIES, "other"];

/** Administration, by theme. Core types not listed here end up in "Other". */
export interface AdminSection {
  id: string;
  label: string;
  description: string;
  items: NavItem[];
}

const ADMIN_SECTIONS: {id: string; label: string; description: string; types: string[]}[] = [
  {
    id: "access",
    label: "Access control",
    description: "Who can sign in, and what each user may see and do.",
    types: ["users", "groups", "roles", "access_policies", "domains"],
  },
  {
    id: "server",
    label: "Server",
    description: "Workers, scheduled work, replication, signing and server-wide operations.",
    types: ["workers", "task-schedules", "task-groups", "upstream-pulps", "signing-services", "operations"],
  },
  {
    id: "storage",
    label: "Storage",
    description: "Files and versions kept by the server.",
    types: ["storage", "artifacts", "uploads", "repository_versions"],
  },
];

/** What each administration page holds (sidebar tooltips, administration page, list subtitles). */
export const ADMIN_DESCRIPTIONS: Record<string, string> = {
  users: "Accounts that can sign in to the API.",
  groups: "Sets of users sharing the same roles.",
  roles: "Named sets of permissions, given to users and groups.",
  access_policies: "Rules deciding who may call each endpoint.",
  domains: "Isolated tenants, each with its own content and storage.",
  workers: "Processes running the tasks.",
  "task-schedules": "Tasks the server runs periodically.",
  "task-groups": "Tasks started together and followed as one.",
  "upstream-pulps": "Other Pulp servers replicated into this one.",
  "signing-services": "Scripts signing metadata and packages.",
  operations: "Orphan cleanup, storage repair, task purge…",
  storage: "Space used by each plugin and repository, and what can be reclaimed.",
  artifacts: "Files stored by the server, by checksum.",
  uploads: "Chunked uploads not yet turned into content.",
  repository_versions: "Every version of every repository.",
};

function order(t: ResourceType): number {
  const i = CATEGORY_ORDER.indexOf(t.category);
  return i < 0 ? CATEGORY_ORDER.length : i;
}

function item(ctx: string, t: ResourceType, label = typeLabel(t)): NavItem {
  return {id: t.id, label, to: listUrl(ctx, t.id), category: t.category, variant: label.includes(" · ")};
}

/** Categories of a plugin listed in simple mode. */
const SIMPLE_CATEGORIES = ["repositories", "remotes", "distributions"];

/**
 * Simple mode: dashboard and tasks, then the repositories, remotes and distributions of each
 * plugin (main types only). Content is reached from repositories; the rest is in advanced mode.
 */
export function simplifyNav(nav: ReturnType<typeof buildNav>): ReturnType<typeof buildNav> {
  const plugins = nav.plugins
    .map((g) => {
      const kept = g.items.filter((i) => SIMPLE_CATEGORIES.includes(i.category ?? ""));
      const items = kept.filter((i) => !i.variant || !kept.some((o) => o.category === i.category && !o.variant));
      return {...g, items};
    })
    .filter((g) => g.id !== "core" && g.items.length > 0);
  return {overview: nav.overview, plugins, platform: [], admin: []};
}

export function buildNav(reg: Registry, ctx: string): {overview: NavItem[]; plugins: NavGroup[]; platform: NavItem[]; admin: AdminSection[]} {
  const top = reg.topLevel().filter((t) => t.list_path);
  const overview: NavItem[] = [
    {id: "dashboard", label: "Dashboard", to: base(ctx)},
    {id: "tasks", label: "Tasks", to: listUrl(ctx, "tasks")},
  ];
  const plugins: NavGroup[] = reg.plugins().map((plugin) => ({
    id: plugin,
    label: pluginLabel(plugin),
    items: top
      .filter((t) => t.plugin === plugin)
      .sort((a, b) => order(a) - order(b) || typeLabel(a).localeCompare(typeLabel(b)))
      .map((t) => item(ctx, t)),
  }));
  // Cross-plugin lists (every repository, remote… whatever its plugin).
  const platform: NavItem[] = top
    .filter((t) => t.generic)
    .sort((a, b) => order(a) - order(b))
    .map((t) => ({...item(ctx, t, categoryLabel(t.id)), category: t.id}));
  // Core types of plugin categories (OpenPGP keyrings, Pulp exporters…): a plugin group of their own.
  const coreItems = top
    .filter((t) => !t.generic && t.plugin === "core" && PLUGIN_CATEGORIES.includes(t.category))
    .sort((a, b) => order(a) - order(b) || typeLabel(a).localeCompare(typeLabel(b)))
    .map((t) => item(ctx, t));
  if (coreItems.length) plugins.push({id: "core", label: "Pulp core", items: coreItems});
  const adminTypes = top.filter(
    (t) => t.plugin === "core" && !t.generic && !PLUGIN_CATEGORIES.includes(t.category) && t.id !== "tasks",
  );
  // Administration pages that are not resource lists.
  const pages: Record<string, NavItem> = {
    operations: {id: "operations", label: "Maintenance", to: `${base(ctx)}/operations`},
    storage: {id: "storage", label: "Disk usage", to: `${base(ctx)}/storage`},
  };
  const admin: AdminSection[] = ADMIN_SECTIONS.map(({types, ...section}) => ({
    ...section,
    items: types.flatMap((id) => {
      if (pages[id]) return [pages[id]];
      const t = adminTypes.find((a) => a.id === id);
      return t ? [item(ctx, t)] : [];
    }),
  }));
  const known = ADMIN_SECTIONS.flatMap((s) => s.types);
  const other = adminTypes.filter((t) => !known.includes(t.id)).sort((a, b) => a.id.localeCompare(b.id));
  if (other.length) {
    admin.push({id: "other", label: "Other", description: "Further server resources.", items: other.map((t) => item(ctx, t))});
  }
  return {overview, plugins, platform, admin: admin.filter((s) => s.items.length > 0)};
}

/** Every destination, for the command palette. */
export function allDestinations(reg: Registry, ctx: string): NavItem[] {
  const nav = buildNav(reg, ctx);
  return [
    ...nav.overview,
    ...nav.plugins.flatMap((g) => g.items.map((i) => ({...i, label: `${g.label} · ${i.label}`, hint: g.label}))),
    ...nav.platform.map((i) => ({...i, label: `All ${i.label.toLowerCase()}`, hint: "All plugins"})),
    {id: "admin", label: "Administration", to: adminUrl(ctx)},
    ...nav.admin.flatMap((s) => s.items.map((i) => ({...i, hint: `Administration · ${s.label}`}))),
    {id: "me", label: "My account & permissions", to: `${base(ctx)}/me`},
  ];
}

/** Administration section holding a type (or `operations`), if any. */
export function adminSection(reg: Registry, ctx: string, id: string): AdminSection | undefined {
  return buildNav(reg, ctx).admin.find((s) => s.items.some((i) => i.id === id));
}
