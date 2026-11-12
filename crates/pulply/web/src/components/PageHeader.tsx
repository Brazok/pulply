import {Breadcrumbs} from "@heroui/react";
import type {ReactNode} from "react";

export interface Crumb {
  label: string;
  to?: string;
}

export function PageHeader({
  crumbs,
  title,
  subtitle,
  chips,
  actions,
}: {
  crumbs?: Crumb[];
  title: ReactNode;
  subtitle?: ReactNode;
  chips?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2">
      {crumbs && crumbs.length > 0 && (
        <Breadcrumbs>
          {crumbs.map((c, i) => (
            <Breadcrumbs.Item key={i} href={c.to}>
              {c.label}
            </Breadcrumbs.Item>
          ))}
        </Breadcrumbs>
      )}
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-1">
          <div className="flex flex-wrap items-center gap-2">
            <h1 className="truncate text-2xl font-semibold tracking-tight">{title}</h1>
            {chips}
          </div>
          {subtitle && <div className="text-sm text-muted">{subtitle}</div>}
        </div>
        {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
      </div>
    </div>
  );
}
