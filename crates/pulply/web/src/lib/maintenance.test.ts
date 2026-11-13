import {describe, expect, it} from "vitest";

import {cutoff, isActive, operationGroup, purgeStates} from "./maintenance";

describe("maintenance", () => {
  it("groups the global operations", () => {
    expect(operationGroup("orphans/cleanup/")).toBe("dedicated");
    expect(operationGroup("tasks/purge/")).toBe("dedicated");
    expect(operationGroup("rpm/prune/")).toBe("maintenance");
    expect(operationGroup("datarepair/7272/")).toBe("maintenance");
    expect(operationGroup("rpm/copy/")).toBe("other");
    expect(operationGroup("rpm/comps/")).toBe("other");
  });

  it("purges only final states", () => {
    expect(purgeStates(["waiting", "running", "completed", "failed", "canceled", "skipped", "canceling"])).toEqual([
      "completed",
      "failed",
      "canceled",
      "skipped",
    ]);
    expect(purgeStates([])).toEqual(["completed", "failed", "canceled"]);
  });

  it("dates the purge cutoff", () => {
    expect(cutoff(30, Date.UTC(2026, 10, 13, 12))).toBe("2026-10-14");
  });

  it("knows unfinished tasks", () => {
    expect(isActive("running")).toBe(true);
    expect(isActive("waiting")).toBe(true);
    expect(isActive("completed")).toBe(false);
    expect(isActive(undefined)).toBe(false);
  });
});
