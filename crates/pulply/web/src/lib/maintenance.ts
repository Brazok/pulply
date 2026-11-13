// Server-wide maintenance operations: which global operations are maintenance (as opposed
// to content operations like `rpm/copy/`), the task each one starts, and the presets the
// Maintenance page offers so the common cases need no form.

/** Operations with a dedicated card, by path relative to the API prefix. */
export const MAINTENANCE = {
  orphans: "orphans/cleanup/",
  reclaim: "repositories/reclaim_space/",
  repair: "repair/",
  purge: "tasks/purge/",
} as const;

export type MaintenanceOp = (typeof MAINTENANCE)[keyof typeof MAINTENANCE];

/** Part of the task name each operation starts (`pulpcore.app.tasks.orphan.orphan_cleanup`). */
export const TASK_NAMES: Record<MaintenanceOp, string> = {
  "orphans/cleanup/": "orphan_cleanup",
  "repositories/reclaim_space/": "reclaim_space",
  "repair/": "repair_all_artifacts",
  "tasks/purge/": "tasks.purge.purge",
};

/** Further operations that maintain the server rather than manage content. */
const OTHER_MAINTENANCE = [/(^|\/)prune\/$/, /^datarepair\//, /^domains\/migrate\/$/];

/** Order of the global operations: the dedicated ones, other maintenance, then the rest. */
export function operationGroup(rel: string): "dedicated" | "maintenance" | "other" {
  if ((Object.values(MAINTENANCE) as string[]).includes(rel)) return "dedicated";
  return OTHER_MAINTENANCE.some((r) => r.test(rel)) ? "maintenance" : "other";
}

/** Ages offered by the task purge, in days. */
export const PURGE_AGES = [7, 30, 90, 365];

/** Final task states a purge may delete: those of the server's enum, else Pulp's. */
export function purgeStates(choices: string[]): string[] {
  const final = ["completed", "failed", "canceled", "skipped"];
  const known = choices.filter((c) => final.includes(c));
  return known.length ? known : ["completed", "failed", "canceled"];
}

/** `YYYY-MM-DD` of `days` ago (UTC): the format `finished_before` and `finished_at__lt` take. */
export function cutoff(days: number, now = Date.now()): string {
  return new Date(now - days * 86_400_000).toISOString().slice(0, 10);
}

/** The task has not finished yet. */
export function isActive(state: unknown): boolean {
  return state === "waiting" || state === "running" || state === "canceling";
}
