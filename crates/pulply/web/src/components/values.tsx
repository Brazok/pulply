// Rendering of Pulp values: hrefs as named links, timestamps, labels, nested JSON.

import {Chip, Tooltip} from "@heroui/react";
import {Link as RouterLink} from "react-router";

import type {Json} from "../lib/api";
import {TASK_COLORS, bytes, dateTime, relativeTime, shortHref} from "../lib/format";
import {isHref, versionNumber, versionRepository} from "../lib/registry";
import {itemUrl} from "../lib/routes";
import {useHrefName, usePulp, useSession} from "../lib/session";

const ISO = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/;

export function HrefLink({href, label, className}: {href: string; label?: string; className?: string}) {
  const {ctx} = useSession();
  const n = versionNumber(href);
  const repo = n !== null ? versionRepository(href) : null;
  const name = useHrefName(label ? null : (repo ?? href));
  const text = label ?? (repo ? `${name ?? shortHref(repo)} · v${n}` : (name ?? shortHref(href)));
  return (
    <RouterLink
      className={`text-accent hover:underline underline-offset-2 break-all ${className ?? ""}`}
      title={href}
      to={itemUrl(ctx, href)}
    >
      {text}
    </RouterLink>
  );
}

/** PRN (`prn:file.filerepository:<id>`) resolved to its resource through `?prn__in=`. */
export function PrnLink({prn}: {prn: string}) {
  const {registry} = useSession();
  const type = registry.typeForPrn(prn);
  const found = usePulp<{results: Json[]}>(
    type?.list_params.some((p) => p.name === "prn__in") ? type.list_path : null,
    {prn__in: prn, fields: "pulp_href"},
  );
  const href = found.data?.results[0]?.pulp_href;
  if (typeof href === "string") return <HrefLink href={href} />;
  return <span className="font-mono text-xs text-muted break-all">{prn}</span>;
}

export function TimeAgo({ts}: {ts: string | null | undefined}) {
  if (!ts) return <span className="text-muted">—</span>;
  return (
    <Tooltip delay={300}>
      <Tooltip.Trigger>
        <span className="cursor-default whitespace-nowrap">{relativeTime(ts)}</span>
      </Tooltip.Trigger>
      <Tooltip.Content>{dateTime(ts)}</Tooltip.Content>
    </Tooltip>
  );
}

export function StateChip({state}: {state: unknown}) {
  const s = String(state ?? "");
  return (
    <Chip color={TASK_COLORS[s] ?? "default"} size="sm" variant="soft">
      {s || "—"}
    </Chip>
  );
}

export function Labels({labels}: {labels: unknown}) {
  const entries = Object.entries((labels as Record<string, string>) ?? {});
  if (!entries.length) return <span className="text-muted">—</span>;
  return (
    <div className="flex flex-wrap gap-1">
      {entries.map(([k, v]) => (
        <Chip key={k} size="sm" variant="secondary">
          {v ? `${k}=${v}` : k}
        </Chip>
      ))}
    </div>
  );
}

/** Any field value, by name and shape. */
export function Value({name, value}: {name?: string; value: unknown}) {
  if (value === null || value === undefined || value === "") return <span className="text-muted">—</span>;
  if (name === "pulp_labels") return <Labels labels={value} />;
  if (name === "state") return <StateChip state={value} />;
  if (typeof value === "boolean") {
    return (
      <Chip color={value ? "success" : "default"} size="sm" variant="soft">
        {value ? "Yes" : "No"}
      </Chip>
    );
  }
  if (typeof value === "number") {
    return <span>{name === "size" || name?.endsWith("_size") ? bytes(value) : value.toLocaleString()}</span>;
  }
  if (typeof value === "string") {
    if (isHref(value)) return <HrefLink href={value} />;
    if (value.startsWith("prn:") && name !== "prn") return <PrnLink prn={value} />;
    if (ISO.test(value)) return <TimeAgo ts={value} />;
    if (/^https?:\/\//.test(value)) {
      return (
        <a className="text-accent hover:underline break-all" href={value} rel="noreferrer" target="_blank">
          {value}
        </a>
      );
    }
    return <span className="break-all whitespace-pre-wrap">{value}</span>;
  }
  if (Array.isArray(value)) {
    if (!value.length) return <span className="text-muted">—</span>;
    if (value.every((v) => typeof v === "string" && !isHref(v)) && value.length <= 12) {
      return (
        <div className="flex flex-wrap gap-1">
          {value.map((v) => (
            <Chip key={v as string} size="sm" variant="secondary">
              {v as string}
            </Chip>
          ))}
        </div>
      );
    }
    if (value.every(isHref)) {
      return (
        <ul className="flex flex-col gap-0.5">
          {value.slice(0, 50).map((h) => (
            <li key={h as string}>
              <HrefLink href={h as string} />
            </li>
          ))}
          {value.length > 50 && <li className="text-muted text-sm">…and {value.length - 50} more</li>}
        </ul>
      );
    }
  }
  return <JsonView value={value} />;
}

/** Collapsible-free JSON tree with clickable hrefs. */
export function JsonView({value}: {value: unknown}) {
  return (
    <pre className="text-xs leading-5 font-mono bg-default/40 rounded-xl p-3 overflow-auto max-h-[480px]">
      <JsonNode value={value} indent={0} />
    </pre>
  );
}

function JsonNode({value, indent}: {value: unknown; indent: number}) {
  const pad = "  ".repeat(indent + 1);
  const close = "  ".repeat(indent);
  if (Array.isArray(value)) {
    if (!value.length) return <>[]</>;
    return (
      <>
        {"[\n"}
        {value.map((v, i) => (
          <span key={i}>
            {pad}
            <JsonNode indent={indent + 1} value={v} />
            {i < value.length - 1 ? ",\n" : "\n"}
          </span>
        ))}
        {close}]
      </>
    );
  }
  if (value && typeof value === "object") {
    const entries = Object.entries(value as Json);
    if (!entries.length) return <>{"{}"}</>;
    return (
      <>
        {"{\n"}
        {entries.map(([k, v], i) => (
          <span key={k}>
            {pad}
            <span className="text-accent">"{k}"</span>: <JsonNode indent={indent + 1} value={v} />
            {i < entries.length - 1 ? ",\n" : "\n"}
          </span>
        ))}
        {close}
        {"}"}
      </>
    );
  }
  if (typeof value === "string") {
    return isHref(value) ? (
      <>
        "<HrefLink href={value} label={value} />"
      </>
    ) : (
      <span className="text-success">{JSON.stringify(value)}</span>
    );
  }
  return <span className="text-warning">{JSON.stringify(value)}</span>;
}
