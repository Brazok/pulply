// Disk usage breakdown, computed by `pulply serve` in the background
// (`/api/contexts/{ctx}/storage`): Pulp has no aggregation endpoint, so the exact numbers
// need a scan of every artifact and content unit, too heavy for the browser on a full server.

import {useMutation, useQuery, useQueryClient} from "@tanstack/react-query";

import {json} from "./api";

export interface Usage {
  size: number;
  count: number;
}

export interface TopItem {
  href: string;
  type_id: string;
  label: string;
  size: number;
}

export interface PluginUsage extends Usage {
  plugin: string;
  types: (Usage & {id: string})[];
  top: TopItem[];
}

export interface RepoUsage extends Usage {
  href: string;
  name: string;
  plugin: string;
}

export interface Report {
  computed_at: string;
  duration_secs: number;
  total: Usage;
  orphaned: Usage;
  unattributed: Usage;
  plugins: PluginUsage[];
  repositories: RepoUsage[];
  repositories_total: number;
  warnings: string[];
}

export interface Progress {
  step: number;
  steps: number;
  label: string;
  done: number;
  total: number;
}

export interface StorageState {
  running: boolean;
  started_at: string | null;
  progress: Progress | null;
  error: string | null;
  report: Report | null;
}

const url = (ctx: string) => `/api/contexts/${encodeURIComponent(ctx)}/storage`;

/**
 * State of the scan, shared by the page and the header: polled every second while it runs,
 * every 5 s otherwise (cheap: answered by pulply), so a scan started elsewhere shows up.
 */
export function useStorageScan(ctx: string) {
  const queryClient = useQueryClient();
  const key = ["storage-scan", ctx];
  const state = useQuery({
    queryKey: key,
    queryFn: () => json<StorageState>(url(ctx)),
    refetchInterval: (q) => (q.state.data?.running ? 1000 : 5000),
  });
  const refresh = () => queryClient.invalidateQueries({queryKey: key});
  const start = useMutation({mutationFn: () => json(url(ctx), {method: "POST"}), onSettled: refresh});
  const cancel = useMutation({mutationFn: () => json(url(ctx), {method: "DELETE"}), onSettled: refresh});
  return {state, start, cancel};
}

/** Share of the scan done, over all its steps (each step counts the same). */
export function scanRatio(p: Progress | null | undefined): number | null {
  if (!p || !p.steps) return null;
  const inStep = p.total ? Math.min(p.done / p.total, 1) : 0;
  return (Math.max(p.step - 1, 0) + inStep) / p.steps;
}
