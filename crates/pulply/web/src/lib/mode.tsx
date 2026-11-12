// Interface mode: "simple" shows the everyday resources and actions, "advanced" every resource
// type, field and operation. Kept in localStorage.

import {useSyncExternalStore} from "react";

export type Mode = "simple" | "advanced";

const KEY = "pulply-mode";
const listeners = new Set<() => void>();

function read(): Mode {
  try {
    return localStorage.getItem(KEY) === "advanced" ? "advanced" : "simple";
  } catch {
    return "simple";
  }
}

let current: Mode = read();

export function setMode(mode: Mode) {
  current = mode;
  try {
    localStorage.setItem(KEY, mode);
  } catch {
    // Storage unavailable: the choice lasts for this page only.
  }
  listeners.forEach((l) => l());
}

export function useMode(): {mode: Mode; simple: boolean; setMode: (m: Mode) => void} {
  const mode = useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => current,
  );
  return {mode, simple: mode === "simple", setMode};
}
