// URLs of the interface. Resources are addressed by their Pulp href.

export const base = (ctx: string) => `/c/${encodeURIComponent(ctx)}`;

export function itemUrl(ctx: string, href: string): string {
  return `${base(ctx)}/item?href=${encodeURIComponent(href)}`;
}

export function listUrl(ctx: string, typeId: string, params?: Record<string, string>): string {
  const q = params && Object.keys(params).length ? `?${new URLSearchParams(params)}` : "";
  return `${base(ctx)}/browse/${typeId}${q}`;
}
