// Connected context: session info, registry, Pulp client, permissions and the tasks
// launched from the interface (followed until they finish).

import {toast} from "sonner";
import {useQuery, useQueryClient} from "@tanstack/react-query";
import {createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useRef, useState} from "react";

import {ApiError, type Json, Pulp, hasToken, json, postJson, taskOf} from "./api";
import {taskName} from "./format";
import {Registry, type RegistryExport, type ResourceType, modelOf} from "./registry";

export interface ContextInfo {
  name: string;
  base_url: string;
  origin: "pulply" | "pulp-cli";
  username: string | null;
  needs_password: boolean;
  readonly: boolean;
  domain: string | null;
  connected: boolean;
}

export interface ContextsResponse {
  contexts: ContextInfo[];
  default: string | null;
  refresh: number;
  upload_chunk_size: number;
}

export interface SessionInfo {
  context: string;
  base_url: string;
  api_prefix: string;
  status_path: string;
  domain: string | null;
  ignored_domain: string | null;
  readonly: boolean;
  username: string | null;
  access: "all" | "limited" | "objects";
  permissions: string[];
  versions: {component: string; version: string}[];
  domain_enabled: boolean;
}

export function useContexts() {
  return useQuery({
    queryKey: ["contexts"],
    queryFn: () => json<ContextsResponse>("/api/contexts"),
    staleTime: 30_000,
    enabled: hasToken(),
    retry: (count, e) => !(e instanceof ApiError && e.status === 401) && count < 1,
  });
}

export function connect(ctx: string, password?: string): Promise<SessionInfo> {
  return postJson<SessionInfo>(`/api/contexts/${encodeURIComponent(ctx)}/connect`, password ? {password} : {});
}

export interface Tracked {
  href: string;
  label: string;
  state: string;
  progress?: string;
  /** Called once the task completed (created resources, error…). */
  onDone?: (task: Json) => void;
}

interface SessionValue {
  ctx: string;
  info: SessionInfo;
  registry: Registry;
  pulp: Pulp;
  refresh: number;
  chunkSize: number;
  readonly: boolean;
  /** May the user `action` (`add`, `change`, `delete`, `sync`…) on type `t`? Object permissions refine it on detail pages. */
  can: (action: string, t: ResourceType | undefined, objectPerms?: string[] | null) => boolean;
  tracked: Tracked[];
  /** Follows the task of an asynchronous response; returns whether there was one. */
  track: (response: unknown, label: string, onDone?: (task: Json) => void) => boolean;
  reloadRegistry: () => Promise<void>;
}

const Ctx = createContext<SessionValue | null>(null);

export function useSession(): SessionValue {
  const v = useContext(Ctx);
  if (!v) throw new Error("useSession outside SessionProvider");
  return v;
}

const DONE = new Set(["completed", "failed", "canceled", "skipped"]);

function codename(action: string, model: string): string | null {
  const [app, name] = model.split(".", 2);
  return name ? `${app}.${action}_${name}` : null;
}

export function SessionProvider(props: {
  ctx: string;
  info: SessionInfo;
  registry: RegistryExport;
  contexts: ContextsResponse;
  children: ReactNode;
}) {
  const {ctx, info, contexts} = props;
  const queryClient = useQueryClient();
  const registry = useMemo(() => new Registry(props.registry), [props.registry]);
  const pulp = useMemo(() => new Pulp(ctx), [ctx]);
  const [tracked, setTracked] = useState<Tracked[]>([]);
  const trackedRef = useRef(tracked);
  trackedRef.current = tracked;

  const can = useCallback(
    (action: string, t: ResourceType | undefined, objectPerms?: string[] | null) => {
      if (info.readonly && action !== "view") return false;
      if (info.access === "all") return true;
      const model = modelOf(t);
      const perm = model ? codename(action, model) : null;
      if (!perm) return info.access === "objects";
      if (objectPerms?.includes(perm)) return true;
      if (info.access === "limited") return info.permissions.includes(perm);
      // Global permissions unknown: creation stays offered, object actions need object perms.
      return action === "add" || objectPerms == null;
    },
    [info],
  );

  const track = useCallback((response: unknown, label: string, onDone?: (task: Json) => void) => {
    const href = taskOf(response);
    if (!href) return false;
    setTracked((ts) => [...ts.filter((t) => t.href !== href), {href, label, state: "waiting", onDone}]);
    toast(label, {description: "Task started", duration: 2500});
    return true;
  }, []);

  // Poll the followed tasks.
  useEffect(() => {
    if (!tracked.length) return;
    const timer = window.setInterval(async () => {
      const current = trackedRef.current;
      const results = await Promise.allSettled(
        current.map((t) => pulp.get(t.href, {fields: "pulp_href,state,name,error,progress_reports,created_resources"})),
      );
      const finished: [Tracked, Json][] = [];
      const next: Tracked[] = [];
      results.forEach((r, i) => {
        const t = current[i];
        if (r.status === "rejected") {
          finished.push([t, {state: "failed", error: {description: String(r.reason)}}]);
          return;
        }
        const task = r.value;
        const state = String(task.state ?? "");
        const reports = (task.progress_reports as Json[] | undefined) ?? [];
        const progress = reports
          .filter((p) => p.state === "running" && p.total)
          .map((p) => `${p.message} ${p.done}/${p.total}`)[0];
        if (DONE.has(state)) finished.push([t, task]);
        else next.push({...t, state, progress});
      });
      setTracked((ts) => {
        const done = new Set(finished.map(([t]) => t.href));
        const updated = new Map(next.map((t) => [t.href, t]));
        return ts.filter((t) => !done.has(t.href)).map((t) => updated.get(t.href) ?? t);
      });
      for (const [t, task] of finished) {
        const state = String(task.state);
        if (state === "completed") toast.success(`${t.label} completed`, {description: taskName(task.name)});
        else {
          const err = task.error as Json | undefined;
          toast.error(`${t.label} ${state}`, {description: String(err?.description ?? ""), duration: 10_000});
        }
        t.onDone?.(task);
      }
      if (finished.length) queryClient.invalidateQueries({queryKey: ["pulp", ctx]});
    }, 1500);
    return () => window.clearInterval(timer);
  }, [tracked.length > 0, pulp, ctx, queryClient]);

  const reloadRegistry = useCallback(async () => {
    const data = await postJson<RegistryExport>(`/api/contexts/${encodeURIComponent(ctx)}/registry/refresh`, {});
    queryClient.setQueryData(["registry", ctx], data);
  }, [ctx, queryClient]);

  const value: SessionValue = {
    ctx,
    info,
    registry,
    pulp,
    refresh: Math.max(1, contexts.refresh) * 1000,
    chunkSize: contexts.upload_chunk_size,
    readonly: info.readonly,
    can,
    tracked,
    track,
    reloadRegistry,
  };
  return <Ctx.Provider value={value}>{props.children}</Ctx.Provider>;
}

/** Query on the Pulp proxy, keyed per context so a finished task refreshes it. */
export function usePulp<T = Json>(
  href: string | null | undefined,
  params?: Record<string, string | number | boolean | undefined>,
  options: {refetchInterval?: number | false; enabled?: boolean} = {},
) {
  const {pulp, ctx} = useSession();
  return useQuery({
    queryKey: ["pulp", ctx, href, params ?? null],
    queryFn: () => pulp.get<T>(href!, params),
    enabled: !!href && options.enabled !== false,
    refetchInterval: options.refetchInterval,
  });
}

/** Display name of an href (cached per context). */
export function useHrefName(href: string | null | undefined): string | undefined {
  const {pulp, ctx} = useSession();
  const {data} = useQuery({
    queryKey: ["name", ctx, href],
    queryFn: async () => {
      const item = await pulp.get(href!, {fields: "pulp_href,name,username,base_path,relative_path,number,repository"});
      for (const k of ["name", "username", "base_path", "relative_path"]) {
        if (typeof item[k] === "string" && item[k]) return item[k] as string;
      }
      return null;
    },
    enabled: !!href,
    staleTime: 5 * 60_000,
    retry: false,
  });
  return data ?? undefined;
}

export function errorText(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 403 && e.code === "readonly") return "This context is read-only.";
    if (e.status === 403) return e.message || "You do not have permission to do this.";
    return e.message;
  }
  return e instanceof Error ? e.message : String(e);
}
