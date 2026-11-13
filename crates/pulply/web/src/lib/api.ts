// HTTP layer: the pulply server API (`/api/...`) and the Pulp proxy
// (`/api/contexts/{ctx}/pulp/<href>`). Every request carries the access token.

const TOKEN_KEY = "pulply.token";

/** Token from the page (loopback), the URL fragment (`#token=`), or a previous visit. */
function initialToken(): string | null {
  const meta = document.querySelector<HTMLMetaElement>('meta[name="pulply-token"]')?.content;
  if (meta) return meta;
  const match = window.location.hash.match(/token=([^&]+)/);
  if (match) {
    const token = decodeURIComponent(match[1]);
    storeToken(token);
    history.replaceState(null, "", window.location.pathname + window.location.search);
    return token;
  }
  try {
    return localStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

/** Read on first use (not at import: the module is also loaded by tests, without a DOM). */
let token: string | null | undefined;

function currentToken(): string | null {
  if (token === undefined) token = initialToken();
  return token;
}

export function storeToken(value: string) {
  token = value;
  try {
    localStorage.setItem(TOKEN_KEY, value);
  } catch {
    /* private mode: kept in memory */
  }
}

export function hasToken() {
  return !!currentToken();
}

/** Error of the pulply API or relayed from Pulp (DRF validation bodies keep their fields). */
export class ApiError extends Error {
  status: number;
  code: string;
  fields: Record<string, string[]>;

  constructor(status: number, code: string, message: string, fields: Record<string, string[]> = {}) {
    super(message);
    this.status = status;
    this.code = code;
    this.fields = fields;
  }
}

function texts(v: unknown): string[] {
  if (Array.isArray(v)) return v.flatMap(texts);
  if (v && typeof v === "object") return Object.entries(v).flatMap(([k, x]) => texts(x).map((t) => `${k}: ${t}`));
  return v == null ? [] : [String(v)];
}

async function toError(resp: Response): Promise<ApiError> {
  const text = await resp.text();
  let body: unknown = null;
  try {
    body = JSON.parse(text);
  } catch {
    /* not JSON */
  }
  if (body && typeof body === "object" && !Array.isArray(body)) {
    const obj = body as Record<string, unknown>;
    if (typeof obj.error === "string") {
      return new ApiError(resp.status, obj.error, String(obj.detail ?? obj.error));
    }
    const fields = Object.fromEntries(Object.entries(obj).map(([k, v]) => [k, texts(v)]));
    const general = [...(fields.detail ?? []), ...(fields.non_field_errors ?? [])];
    const message =
      general.join(" ") ||
      Object.entries(fields)
        .map(([k, v]) => `${k}: ${v.join(", ")}`)
        .join("; ");
    const code = resp.status === 403 ? "forbidden" : resp.status === 400 ? "validation" : "upstream";
    return new ApiError(resp.status, code, message || `HTTP ${resp.status}`, fields);
  }
  return new ApiError(resp.status, "http", text.slice(0, 300) || `HTTP ${resp.status}`);
}

export async function request(path: string, init: RequestInit = {}): Promise<Response> {
  const headers = new Headers(init.headers);
  const t = currentToken();
  if (t) headers.set("X-Pulply-Token", t);
  const resp = await fetch(path, {...init, headers});
  if (!resp.ok) throw await toError(resp);
  return resp;
}

export async function json<T = unknown>(path: string, init: RequestInit = {}): Promise<T> {
  const resp = await request(path, init);
  const text = await resp.text();
  return (text ? JSON.parse(text) : null) as T;
}

export function postJson<T = unknown>(path: string, body: unknown, method = "POST"): Promise<T> {
  return json<T>(path, {method, headers: {"Content-Type": "application/json"}, body: JSON.stringify(body)});
}

// ---------------------------------------------------------------- Pulp proxy

export type Json = Record<string, unknown>;

export interface Page<T = Json> {
  count: number;
  next: string | null;
  previous: string | null;
  results: T[];
}

export type Params = Record<string, string | number | boolean | undefined | null>;

/** Absolute API path of an href or pagination URL (`http://pulp/pulp/api/v3/x/?a=1` → `/pulp/api/v3/x/?a=1`). */
export function hrefPath(href: string): string {
  if (/^https?:\/\//.test(href)) {
    const url = new URL(href);
    return url.pathname + url.search;
  }
  return href;
}

function withParams(href: string, params?: Params): string {
  const path = hrefPath(href);
  if (!params) return path;
  const [base, query] = path.split("?", 2);
  const search = new URLSearchParams(query);
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== null && v !== "") search.set(k, String(v));
  }
  const q = search.toString();
  return q ? `${base}?${q}` : base;
}

/** Body sent to Pulp: JSON, or a multipart form (files, endpoints refusing JSON). */
export interface Body {
  value: Json;
  files?: Record<string, File>;
  multipart?: boolean;
}

function formData(body: Body): FormData {
  const form = new FormData();
  for (const [k, v] of Object.entries(body.value)) {
    if (v === undefined || v === null) continue;
    if (Array.isArray(v)) v.forEach((item) => form.append(k, typeof item === "string" ? item : JSON.stringify(item)));
    else if (typeof v === "object") form.append(k, JSON.stringify(v));
    else form.append(k, String(v));
  }
  for (const [k, f] of Object.entries(body.files ?? {})) form.append(k, f, f.name);
  return form;
}

export class Pulp {
  readonly ctx: string;

  constructor(ctx: string) {
    this.ctx = ctx;
  }

  url(href: string, params?: Params): string {
    return `/api/contexts/${encodeURIComponent(this.ctx)}/pulp${withParams(href, params)}`;
  }

  get<T = Json>(href: string, params?: Params): Promise<T> {
    return json<T>(this.url(href, params));
  }

  list<T = Json>(href: string, params?: Params): Promise<Page<T>> {
    return json<Page<T>>(this.url(href, params));
  }

  /** Every page of a list (bounded), for pickers and small collections. */
  async all<T = Json>(href: string, params?: Params, max = 1000): Promise<T[]> {
    const out: T[] = [];
    let next: string | null = this.url(href, {limit: 200, ...params});
    while (next && out.length < max) {
      const page: Page<T> = await json<Page<T>>(next);
      out.push(...page.results);
      next = page.next ? this.url(page.next) : null;
    }
    return out;
  }

  send<T = Json>(method: string, href: string, body?: Body | null): Promise<T> {
    if (!body) return json<T>(this.url(href), {method});
    if (body.multipart || (body.files && Object.keys(body.files).length > 0)) {
      return json<T>(this.url(href), {method, body: formData(body)});
    }
    return json<T>(this.url(href), {
      method,
      headers: {"Content-Type": "application/json"},
      body: JSON.stringify(body.value),
    });
  }

  post<T = Json>(href: string, value: Json = {}): Promise<T> {
    return this.send<T>("POST", href, {value});
  }

  patch<T = Json>(href: string, value: Json): Promise<T> {
    return this.send<T>("PATCH", href, {value});
  }

  delete(href: string): Promise<unknown> {
    return this.send("DELETE", href);
  }
}

/** Task href of an asynchronous response (`{"task": …}`), task group excluded. */
export function taskOf(response: unknown): string | null {
  if (response && typeof response === "object" && "task" in response) {
    const t = (response as Json).task;
    return typeof t === "string" ? t : null;
  }
  return null;
}
