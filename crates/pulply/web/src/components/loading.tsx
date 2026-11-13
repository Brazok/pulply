// Loading states, following common practice (Nielsen's 0.1 / 1 / 10 s limits, skeleton vs
// spinner guidance of NN/g and design systems such as Stripe's):
// - nothing for the first ~300 ms, so fast answers do not flash a loader;
// - a skeleton when the layout is known (pages, lists), a spinner for small actions;
// - past ~8 s, say that the server is slow and how long it has been waiting;
// - long operations with a measurable end use progress bars (scan, uploads, tasks).

import {Hourglass} from "lucide-react";
import {type ReactNode, useEffect, useState} from "react";

import {Skeleton} from "./ui/skeleton";
import {Spinner} from "./ui/spinner";
import {TableCell, TableRow} from "./ui/table";

/** Delay before a loader shows up. */
export const LOADER_DELAY = 300;
/** Wait after which the slowness is acknowledged. */
export const SLOW_AFTER = 8_000;

/** True once `active` has lasted `ms`. */
export function useDelayed(active: boolean, ms = LOADER_DELAY): boolean {
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (!active) {
      setShown(false);
      return;
    }
    const t = setTimeout(() => setShown(true), ms);
    return () => clearTimeout(t);
  }, [active, ms]);
  return active && shown;
}

/** Seconds since the component mounted, ticking every second. */
function useElapsed(): number {
  const [start] = useState(() => Date.now());
  const [now, setNow] = useState(start);
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  return Math.floor((now - start) / 1000);
}

/** Renders `children` only after `ms` (fast loads show nothing at all). */
export function Delayed({children, ms = LOADER_DELAY}: {children: ReactNode; ms?: number}) {
  return useDelayed(true, ms) ? <>{children}</> : null;
}

/** "Still waiting for the server…" once a load lasts more than `SLOW_AFTER`. */
export function SlowHint({what = "the server", className}: {what?: string; className?: string}) {
  const elapsed = useElapsed();
  if (elapsed * 1000 < SLOW_AFTER) return null;
  return (
    <p aria-live="polite" className={`flex items-center gap-2 text-sm text-muted-foreground ${className ?? ""}`}>
      <Hourglass className="size-4 shrink-0" />
      Still waiting for {what} ({elapsed} s). Large servers can take a while; the page fills in as soon as the answer comes.
    </p>
  );
}

/** Centered spinner for whole-screen waits (connection, schema), delayed, with the slow hint. */
export function PageSpinner({label}: {label?: string}) {
  return (
    <Delayed>
      <div className="flex flex-col items-center gap-3 text-center">
        <Spinner className="size-8 text-muted-foreground" />
        {label && <span className="text-sm text-muted-foreground">{label}</span>}
        <SlowHint className="max-w-sm" />
      </div>
    </Delayed>
  );
}

/** Shape of a resource page (header, actions, tabs, two cards) while it loads. */
export function DetailSkeleton() {
  return (
    <Delayed>
      <div aria-busy="true" aria-label="Loading" className="flex flex-col gap-6">
        <div className="flex flex-col gap-3">
          <Skeleton className="h-4 w-48" />
          <div className="flex items-center gap-2">
            <Skeleton className="h-8 w-64" />
            <Skeleton className="h-5 w-16 rounded-full" />
          </div>
          <Skeleton className="h-3.5 w-96 max-w-full" />
          <div className="flex gap-2">
            {[20, 24, 28, 16].map((w, i) => (
              <Skeleton key={i} className="h-9" style={{width: `${w * 4}px`}} />
            ))}
          </div>
        </div>
        <div className="flex gap-4 border-b pb-2">
          {[16, 12, 16, 14, 12].map((w, i) => (
            <Skeleton key={i} className="h-5" style={{width: `${w * 4}px`}} />
          ))}
        </div>
        {[4, 6].map((rows, c) => (
          <div key={c} className="flex flex-col gap-4 rounded-xl border p-6">
            <Skeleton className="h-5 w-32" />
            <div className="grid grid-cols-1 gap-x-10 gap-y-4 md:grid-cols-2">
              {Array.from({length: rows}, (_, i) => (
                <div key={i} className="flex items-center gap-6">
                  <Skeleton className="h-4 w-28 shrink-0" />
                  <Skeleton className="h-4 flex-1" />
                </div>
              ))}
            </div>
          </div>
        ))}
        <SlowHint />
      </div>
    </Delayed>
  );
}

/** Placeholder rows of a table, in its real columns, during the first load. */
export function TableSkeletonRows({columns, rows = 8}: {columns: number; rows?: number}) {
  const shown = useDelayed(true);
  if (!shown) return null;
  return (
    <>
      {Array.from({length: rows}, (_, r) => (
        <TableRow key={r} aria-hidden className="hover:bg-transparent">
          {Array.from({length: columns}, (_, c) => (
            <TableCell key={c}>
              {/* Varied widths read as text, not as a grid of bars. */}
              <Skeleton className="h-4" style={{width: `${c === 0 ? 70 - ((r * 13) % 30) : 40 + ((r * 7 + c * 11) % 45)}%`}} />
            </TableCell>
          ))}
        </TableRow>
      ))}
      <TableRow className="hover:bg-transparent">
        <TableCell className="border-0 p-0" colSpan={columns}>
          <SlowHint className="px-3 py-3" />
        </TableCell>
      </TableRow>
    </>
  );
}

/** Lines of text while a small panel loads (roles, summaries). */
export function LinesSkeleton({lines = 3}: {lines?: number}) {
  return (
    <Delayed>
      <div aria-busy="true" className="flex flex-col gap-2.5">
        {Array.from({length: lines}, (_, i) => (
          <Skeleton key={i} className="h-4" style={{width: `${90 - i * 15}%`}} />
        ))}
      </div>
    </Delayed>
  );
}
