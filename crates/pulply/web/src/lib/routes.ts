// URLs of the interface, kept readable: a resource is addressed by its Pulp href without the
// API prefix (`/c/lab/repositories/file/file/0192…/`), and query strings leave `/`, `:` and
// `,` unescaped (they are valid there), so hrefs in filters stay legible.

export const base = (ctx: string) => `/c/${encodeURIComponent(ctx)}`;

export const adminUrl = (ctx: string) => `${base(ctx)}/admin`;

/** Pages of the interface itself; any other path under a context is a resource. */
export const PAGES = ["admin", "browse", "item", "me", "operations", "storage"];

const API_PREFIX = /^\/pulp\/(?:[^/]+\/)?api\/v3\//;

const enc = (s: string) =>
  encodeURIComponent(s).replace(/%2F/gi, "/").replace(/%3A/gi, ":").replace(/%2C/gi, ",").replace(/%40/g, "@");

/** `?a=1&href=/pulp/api/v3/x/`, or "" without parameters. */
export function queryString(params: Record<string, string> | URLSearchParams): string {
  const entries = params instanceof URLSearchParams ? [...params.entries()] : Object.entries(params);
  return entries.length ? `?${entries.map(([k, v]) => `${enc(k)}=${enc(v)}`).join("&")}` : "";
}

/** Interface URL of a Pulp href (`/pulp/api/v3/tasks/01…/` → `/c/lab/tasks/01…/`). */
export function itemUrl(ctx: string, href: string): string {
  const [path, query] = href.replace(/^https?:\/\/[^/]+/, "").split("?", 2);
  const rel = path.replace(API_PREFIX, "");
  return `${base(ctx)}/${rel.split("/").map(enc).join("/")}${query ? `?${query}` : ""}`;
}

/**
 * Pulp href of an interface path under a context (`repositories/file/file/01…/`), or null
 * for the interface's own pages.
 */
export function hrefOfPath(rest: string, apiPrefix: string): string | null {
  const clean = rest.replace(/^\/+/, "");
  if (!clean || PAGES.includes(clean.split("/")[0])) return null;
  return `${apiPrefix}${clean.split("/").map(decodeURIComponent).join("/")}`;
}

export function listUrl(ctx: string, typeId: string, params?: Record<string, string>): string {
  return `${base(ctx)}/browse/${typeId}${params ? queryString(params) : ""}`;
}
