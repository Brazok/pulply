// `content_summary` of a repository version: per content type, present / added /
// removed counts linking to the filtered content lists.

import {Skeleton, Table} from "@heroui/react";
import {Link} from "react-router";

import type {Json} from "../../lib/api";
import {titleize, typeTitle} from "../../lib/format";
import {listUrl} from "../../lib/routes";
import {usePulp, useSession} from "../../lib/session";

type Section = Record<string, {count: number; href: string}>;

export function ContentSummary({versionHref}: {versionHref: string}) {
  const {registry, ctx} = useSession();
  const version = usePulp<Json>(versionHref);
  if (version.isLoading) return <Skeleton className="h-20 rounded-xl" />;
  const summary = (version.data?.content_summary ?? {}) as {added?: Section; removed?: Section; present?: Section};
  const kinds = [...new Set([...Object.keys(summary.present ?? {}), ...Object.keys(summary.added ?? {}), ...Object.keys(summary.removed ?? {})])].sort();
  if (!kinds.length) return <p className="text-sm text-muted">This version has no content.</p>;

  const link = (entry: {count: number; href: string} | undefined, className: string) => {
    if (!entry?.count) return <span className="text-muted">0</span>;
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
    <Table variant="secondary">
      <Table.ScrollContainer>
        <Table.Content aria-label="Content summary">
          <Table.Header>
            <Table.Column isRowHeader>Content type</Table.Column>
            <Table.Column>Present</Table.Column>
            <Table.Column>Added</Table.Column>
            <Table.Column>Removed</Table.Column>
          </Table.Header>
          <Table.Body>
            {kinds.map((k) => (
              <Table.Row key={k} id={k}>
                <Table.Cell>{label(k)}</Table.Cell>
                <Table.Cell>{link(summary.present?.[k], "text-accent")}</Table.Cell>
                <Table.Cell>{link(summary.added?.[k], "text-success")}</Table.Cell>
                <Table.Cell>{link(summary.removed?.[k], "text-danger")}</Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}
