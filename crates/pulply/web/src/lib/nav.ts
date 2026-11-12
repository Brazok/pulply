// Navigation tree built from the registry: overview, one group per plugin, platform
// (cross-plugin and core content management) and administration.

import {categoryLabel, pluginLabel, typeLabel} from "./format";
import {PLUGIN_CATEGORIES, type Registry, type ResourceType} from "./registry";
import {base, listUrl} from "./routes";

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
const ADMIN_ORDER = [
  "users",
  "groups",
  "roles",
  "access_policies",
  "domains",
  "upstream-pulps",
  "signing-services",
  "task-schedules",
  "task-groups",
  "workers",
  "artifacts",
  "uploads",
  "repository_versions",
];

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
    .filter((g) => g.items.length > 0);
  return {overview: nav.overview.filter((i) => i.id !== "operations"), plugins, platform: [], admin: []};
}

export function buildNav(reg: Registry, ctx: string) {
  const top = reg.topLevel().filter((t) => t.list_path);
  const overview: NavItem[] = [
    {id: "dashboard", label: "Dashboard", to: base(ctx)},
    {id: "tasks", label: "Tasks", to: listUrl(ctx, "tasks")},
    {id: "operations", label: "Maintenance", to: `${base(ctx)}/operations`},
  ];
  const plugins: NavGroup[] = reg.plugins().map((plugin) => ({
    id: plugin,
    label: pluginLabel(plugin),
    items: top
      .filter((t) => t.plugin === plugin)
      .sort((a, b) => order(a) - order(b) || typeLabel(a).localeCompare(typeLabel(b)))
      .map((t) => item(ctx, t)),
  }));
  // Cross-plugin lists and core types of plugin categories (RBAC guards, Pulp exporters…).
  const platform: NavItem[] = [
    ...top.filter((t) => t.generic).sort((a, b) => order(a) - order(b)).map((t) => item(ctx, t, `All ${categoryLabel(t.id).toLowerCase()}`)),
    ...top
      .filter((t) => !t.generic && t.plugin === "core" && PLUGIN_CATEGORIES.includes(t.category))
      .sort((a, b) => order(a) - order(b))
      .map((t) => item(ctx, t)),
  ];
  const adminTypes = top.filter(
    (t) => t.plugin === "core" && !t.generic && !PLUGIN_CATEGORIES.includes(t.category) && t.id !== "tasks",
  );
  const admin = adminTypes
    .sort((a, b) => {
      const ia = ADMIN_ORDER.indexOf(a.id);
      const ib = ADMIN_ORDER.indexOf(b.id);
      return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib) || a.id.localeCompare(b.id);
    })
    .map((t) => item(ctx, t));
  return {overview, plugins, platform, admin};
}

/** Every destination, for the command palette. */
export function allDestinations(reg: Registry, ctx: string): NavItem[] {
  const nav = buildNav(reg, ctx);
  return [
    ...nav.overview,
    ...nav.plugins.flatMap((g) => g.items.map((i) => ({...i, label: `${g.label} · ${i.label}`, hint: g.label}))),
    ...nav.platform.map((i) => ({...i, hint: "Platform"})),
    ...nav.admin.map((i) => ({...i, hint: "Administration"})),
    {id: "me", label: "My account & permissions", to: `${base(ctx)}/me`},
  ];
}
