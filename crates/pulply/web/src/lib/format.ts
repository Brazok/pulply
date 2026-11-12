import type {Json} from "./api";
import {type ResourceType, isHref} from "./registry";

const CATEGORY_LABELS: Record<string, string> = {
  repositories: "Repositories",
  remotes: "Remotes",
  distributions: "Distributions",
  publications: "Publications",
  content: "Content",
  acs: "Alternate content sources",
  contentguards: "Content guards",
  exporters: "Exporters",
  importers: "Importers",
  tasks: "Tasks",
  "task-groups": "Task groups",
  "task-schedules": "Task schedules",
  workers: "Workers",
  users: "Users",
  groups: "Groups",
  roles: "Roles",
  artifacts: "Artifacts",
  uploads: "Uploads",
  access_policies: "Access policies",
  "signing-services": "Signing services",
  "upstream-pulps": "Upstream Pulps",
  domains: "Domains",
  repository_versions: "Repository versions",
  orphans: "Orphans",
  exports: "Exports",
  imports: "Imports",
  versions: "Versions",
  other: "Other",
};

const ACRONYMS: Record<string, string> = {
  rpm: "RPM",
  deb: "Deb",
  acs: "ACS",
  npm: "npm",
  ostree: "OSTree",
  ansible: "Ansible",
  container: "Container",
  python: "Python",
  maven: "Maven",
  gem: "Gem",
  file: "File",
  certguard: "Certguard",
  hugging_face: "Hugging Face",
  core: "Core",
};

/** `access_policies` → `Access policies`. */
export function titleize(id: string): string {
  const words = id.replace(/[_\-/]/g, " ").trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export function pluginLabel(plugin: string): string {
  return ACRONYMS[plugin] ?? titleize(plugin);
}

export function categoryLabel(id: string): string {
  return CATEGORY_LABELS[id] ?? titleize(id);
}

/** Label of a type inside its plugin group: `Repositories`, `Packages`, `Content guards · RBAC`. */
export function typeLabel(t: ResourceType): string {
  if (t.parent) return titleize(t.name);
  if (t.category === "other") return titleize(t.name);
  if (t.plugin === "core" && !t.id.includes("/")) return categoryLabel(t.id);
  if (t.category === "content") return titleize(t.name);
  const cat = categoryLabel(t.category);
  return t.name === t.plugin ? cat : `${cat} · ${titleize(t.name)}`;
}

/** Standalone title: `RPM repositories`, `File content · files`. */
export function typeTitle(t: ResourceType): string {
  if (t.plugin === "core") return typeLabel(t);
  if (t.category === "content") {
    // `file/files` → "File content", `rpm/packages` → "RPM packages".
    return t.name.startsWith(t.plugin) ? `${pluginLabel(t.plugin)} content` : `${pluginLabel(t.plugin)} ${titleize(t.name).toLowerCase()}`;
  }
  const label = typeLabel(t);
  return `${pluginLabel(t.plugin)} ${label.charAt(0).toLowerCase()}${label.slice(1)}`;
}

export function singular(label: string): string {
  if (label.endsWith("ies")) return label.slice(0, -3) + "y";
  if (label.endsWith("ses")) return label.slice(0, -2);
  if (label.endsWith("s")) return label.slice(0, -1);
  return label;
}

/** `/pulp/api/v3/remotes/file/file/0192abcd-…/` → `remotes/file · 0192abcd`. */
export function shortHref(href: string): string {
  const rel = (href.split("/api/v3/")[1] ?? href).replace(/\/+$/, "");
  const segs = rel.split("/");
  const id = segs.pop() ?? href;
  if (!segs.length) return id;
  return `${segs.slice(0, 2).join("/")} · ${id.slice(0, 8)}`;
}

/** Best display name of a resource. */
export function displayName(item: Json | undefined | null, fallbackHref?: string): string {
  if (item) {
    for (const k of ["name", "username", "base_path", "relative_path", "viewset_name", "role", "number"]) {
      const v = item[k];
      if (typeof v === "string" && v) return v;
      if (k === "number" && typeof v === "number") return `Version ${v}`;
    }
    if (typeof item.pulp_href === "string") return shortHref(item.pulp_href);
  }
  return fallbackHref ? shortHref(fallbackHref) : "";
}

const rtf = new Intl.RelativeTimeFormat(undefined, {numeric: "auto"});

export function relativeTime(ts: string | null | undefined, now = Date.now()): string {
  if (!ts) return "";
  const t = Date.parse(ts);
  if (Number.isNaN(t)) return ts;
  const secs = Math.round((t - now) / 1000);
  const abs = Math.abs(secs);
  if (abs < 60) return rtf.format(secs, "second");
  if (abs < 3600) return rtf.format(Math.round(secs / 60), "minute");
  if (abs < 86400) return rtf.format(Math.round(secs / 3600), "hour");
  if (abs < 30 * 86400) return rtf.format(Math.round(secs / 86400), "day");
  if (abs < 365 * 86400) return rtf.format(Math.round(secs / (30 * 86400)), "month");
  return rtf.format(Math.round(secs / (365 * 86400)), "year");
}

export function dateTime(ts: string | null | undefined): string {
  if (!ts) return "";
  const t = new Date(ts);
  return Number.isNaN(t.getTime()) ? ts : t.toLocaleString();
}

export function duration(start?: string | null, end?: string | null, now = Date.now()): string {
  if (!start) return "";
  const s = Date.parse(start);
  const e = end ? Date.parse(end) : now;
  let secs = Math.max(0, Math.round((e - s) / 1000));
  if (secs < 60) return `${secs}s`;
  const parts: string[] = [];
  for (const [unit, size] of [
    ["d", 86400],
    ["h", 3600],
    ["m", 60],
  ] as const) {
    if (secs >= size) {
      parts.push(`${Math.floor(secs / size)}${unit}`);
      secs %= size;
    }
  }
  if (parts.length < 2 && secs) parts.push(`${secs}s`);
  return parts.slice(0, 2).join(" ");
}

export function bytes(n: number | null | undefined): string {
  if (n == null) return "";
  const units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
  let v = n;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  return u === 0 ? `${n} B` : `${v.toFixed(1)} ${units[u]}`;
}

/** Task name without module path: `pulpcore.app.tasks.base.general_create` → `general create`. */
export function taskName(name: unknown): string {
  if (typeof name !== "string") return "";
  const last = name.split(".").at(-1) ?? name;
  // `ageneral_update` is the async variant of `general_update`.
  return last.replace(/^a(general_)/, "$1").replace(/_/g, " ");
}

export function contentSummary(summary: unknown): {added: number; removed: number; present: number} {
  const total = (key: string) => {
    const section = (summary as Json | undefined)?.[key] as Record<string, {count?: number}> | undefined;
    return Object.values(section ?? {}).reduce((acc, t) => acc + (t?.count ?? 0), 0);
  };
  return {added: total("added"), removed: total("removed"), present: total("present")};
}

/** RPM `name-[epoch:]version-release.arch`. */
export function nevra(item: Json): string {
  const {name, epoch, version, release, arch} = item as Record<string, string | undefined>;
  if (!name || !version) return "";
  const e = epoch && epoch !== "0" ? `${epoch}:` : "";
  return `${name}-${e}${version}-${release ?? ""}${arch ? "." + arch : ""}`;
}

export function scalarText(v: unknown): string {
  if (v === null || v === undefined) return "";
  if (typeof v === "boolean") return v ? "Yes" : "No";
  if (typeof v === "string") return v;
  if (typeof v === "number") return String(v);
  if (Array.isArray(v)) return v.every((x) => typeof x === "string" && !isHref(x)) ? v.join(", ") : `${v.length} items`;
  return JSON.stringify(v);
}

export function fieldLabel(name: string): string {
  const special: Record<string, string> = {
    pulp_href: "Href",
    pulp_created: "Created",
    pulp_last_updated: "Last updated",
    pulp_labels: "Labels",
    prn: "PRN",
    base_url: "Base URL",
    url: "URL",
    sha256: "SHA-256",
    ca_cert: "CA certificate",
    tls_validation: "TLS validation",
  };
  return special[name] ?? titleize(name);
}

export const TASK_COLORS: Record<string, "default" | "accent" | "success" | "warning" | "danger"> = {
  completed: "success",
  running: "accent",
  waiting: "default",
  failed: "danger",
  canceled: "warning",
  canceling: "warning",
  skipped: "default",
};
