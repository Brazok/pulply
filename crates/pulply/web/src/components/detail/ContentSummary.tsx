// `content_summary` of a repository version: per content type, present / added /
// removed counts linking to the filtered content lists.

import {Link} from "react-router";

import type {Json} from "../../lib/api";
import {titleize, typeTitle} from "../../lib/format";
import {listUrl} from "../../lib/routes";
import {usePulp, useSession} from "../../lib/session";
import {Skeleton} from "../ui/skeleton";
import {Table, TableBody, TableCell, TableHead, TableHeader, TableRow} from "../ui/table";

type Section = Record<string, {count: number; href: string}>;

export function ContentSummary({versionHref}: {versionHref: string}) {
  const {registry, ctx} = useSession();
  const version = usePulp<Json>(versionHref);
  if (version.isLoading) return <Skeleton className="h-20" />;
  const summary = (version.data?.content_summary ?? {}) as {added?: Section; removed?: Section; present?: Section};
  const kinds = [...new Set([...Object.keys(summary.present ?? {}), ...Object.keys(summary.added ?? {}), ...Object.keys(summary.removed ?? {})])].sort();
  if (!kinds.length) return <p className="text-sm text-muted-foreground">This version has no content.</p>;

  const link = (entry: {count: number; href: string} | undefined, className: string) => {
    if (!entry?.count) return <span className="text-muted-foreground">0</span>;
    const [path, query] = entry.href.split("?", 2);
    const t = registry.typeForHref(path);
    if (!t) return <span className={className}>{entry.count}</span>;
    return (
      <Link className={`${className} font-medium hover:underline`} to={listUrl(ctx, t.id, Object.fromEntries(new URLSearchParams(query)))}>
        {entry.count.toLocaleString()}
      </Link>
    );
  };
  const label = (kind: string) => {
    const any = summary.present?.[kind] ?? summary.added?.[kind] ?? summary.removed?.[kind];
    const t = any ? registry.typeForHref(any.href.split("?")[0]) : undefined;
    return t ? typeTitle(t) : titleize(kind);
  };
  return (
    <Table aria-label="Content summary">
      <TableHeader>
        <TableRow>
          <TableHead>Content type</TableHead>
          <TableHead>Present</TableHead>
          <TableHead>Added</TableHead>
          <TableHead>Removed</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {kinds.map((k) => (
          <TableRow key={k}>
            <TableCell className="font-medium">{label(k)}</TableCell>
            <TableCell>{link(summary.present?.[k], "text-primary")}</TableCell>
            <TableCell>{link(summary.added?.[k], "text-success")}</TableCell>
            <TableCell>{link(summary.removed?.[k], "text-destructive")}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
