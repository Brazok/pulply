// Light / dark / system theme: the `dark` class on <html>, kept in localStorage.

import {useEffect, useSyncExternalStore} from "react";

export type Theme = "light" | "dark" | "system";

const KEY = "pulply-theme";
const listeners = new Set<() => void>();
const media = window.matchMedia("(prefers-color-scheme: dark)");

function read(): Theme {
  try {
    const v = localStorage.getItem(KEY);
    return v === "light" || v === "dark" ? v : "system";
  } catch {
    return "system";
  }
}

let current: Theme = read();

function apply() {
  const dark = current === "dark" || (current === "system" && media.matches);
  document.documentElement.classList.toggle("dark", dark);
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
}

apply();
media.addEventListener("change", apply);

export function setTheme(theme: Theme) {
  current = theme;
  try {
    if (theme === "system") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, theme);
  } catch {
    // Storage unavailable: the choice lasts for this page only.
  }
  apply();
  listeners.forEach((l) => l());
}

export function useTheme(): {theme: Theme; setTheme: (t: Theme) => void} {
  const theme = useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => current,
  );
  useEffect(apply, [theme]);
  return {theme, setTheme};
}
