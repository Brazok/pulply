// Resource registry built by the pulply server from the OpenAPI schema
// (`pulply_api::Registry::export`), plus the lookups the interface needs.

export type Method = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

export interface Property {
  type?: string;
  format?: string;
  $ref?: string;
  readOnly?: boolean;
  writeOnly?: boolean;
  nullable?: boolean;
  enum?: unknown[];
  default?: unknown;
  description?: string;
  items?: Property;
  allOf?: Property[];
  oneOf?: Property[];
}

export interface Schema {
  type?: string;
  properties?: Record<string, Property>;
  required?: string[];
  enum?: unknown[];
}

export interface Action {
  verb: string;
  method: Method;
  body_schema: string | null;
  response_schema: string | null;
  multipart: boolean;
}

export interface Param {
  name: string;
  ty: "string" | "integer" | "number" | "boolean" | "array";
  enum_values: string[];
  description: string | null;
}

export interface ResourceType {
  id: string;
  category: string;
  plugin: string;
  name: string;
  list_path: string | null;
  parent: [string, string] | null;
  generic: boolean;
  ops: {list: boolean; create: boolean; read: boolean; update: boolean; partial_update: boolean; delete: boolean};
  actions: Action[];
  children: string[];
  list_params: Param[];
  read_schema: string | null;
  create_schema: string | null;
  create_multipart: boolean;
  update_schema: string | null;
}

export interface GlobalOperation {
  path: string;
  body_schema: string | null;
  summary: string | null;
  multipart: boolean;
}

export interface RegistryExport {
  types: ResourceType[];
  schemas: Record<string, Schema>;
  operations: Record<string, GlobalOperation>;
}

/** Categories whose types are `category/plugin/type`. */
export const PLUGIN_CATEGORIES = [
  "repositories",
  "remotes",
  "distributions",
  "publications",
  "content",
  "acs",
  "contentguards",
  "exporters",
  "importers",
];

export function isHref(s: unknown): boolean {
  return typeof s === "string" && s.startsWith("/") && s.includes("/api/v3/") && s.endsWith("/");
}

/** `…/versions/12/` → 12. */
export function versionNumber(href: string): number | null {
  const parts = href.replace(/\/+$/, "").split("/");
  const n = Number(parts.at(-1));
  return parts.at(-2) === "versions" && Number.isInteger(n) ? n : null;
}

/** `…/<uuid>/versions/3/` → `…/<uuid>/`. */
export function versionRepository(href: string): string | null {
  const trimmed = href.replace(/\/+$/, "");
  const idx = trimmed.lastIndexOf("/versions/");
  return idx < 0 ? null : href.slice(0, idx + 1);
}

const strip = (ref: string) => ref.split("/").at(-1) ?? ref;

export class Registry {
  readonly types: ResourceType[];
  readonly schemas: Record<string, Schema>;
  readonly operations: Record<string, GlobalOperation>;
  private byId: Map<string, ResourceType>;
  private listed: ResourceType[];

  constructor(data: RegistryExport) {
    this.types = data.types;
    this.schemas = data.schemas;
    this.operations = data.operations;
    this.byId = new Map(data.types.map((t) => [t.id, t]));
    this.listed = data.types
      .filter((t) => !t.generic && t.list_path)
      .sort((a, b) => b.list_path!.length - a.list_path!.length);
  }

  get(id: string): ResourceType | undefined {
    return this.byId.get(id);
  }

  schema(name: string | null | undefined): Schema | undefined {
    return name ? this.schemas[name] : undefined;
  }

  readSchema(t: ResourceType): Schema | undefined {
    return this.schema(t.read_schema);
  }

  /** Type of an href, versions resolved through their repository. */
  typeForHref(href: string): ResourceType | undefined {
    const repo = versionRepository(href);
    if (repo && versionNumber(href) !== null) {
      const parent = this.typeForHref(repo);
      return parent?.children.map((c) => this.get(c)).find((c) => c?.name === "versions");
    }
    const path = href.split("?")[0];
    return this.listed.find((t) => path.startsWith(t.list_path!));
  }

  /** Top-level type of a PRN (`prn:file.filerepository:<id>`), matched on its read schema. */
  typeForPrn(prn: string): ResourceType | undefined {
    const model = prn.startsWith("prn:") ? prn.split(":")[1] : null;
    if (!model) return undefined;
    return this.types.find(
      (t) => !t.generic && t.list_path && t.read_schema?.toLowerCase() === `${model}response`,
    );
  }

  /** Enum values of a property (inline, `$ref`/`allOf`/`oneOf` to an enum, items of an array). */
  enumChoices(prop: Property): string[] {
    const strings = (values?: unknown[]) => (values ?? []).filter((v): v is string => typeof v === "string");
    if (prop.enum?.length) return strings(prop.enum);
    const refs = [prop.$ref, ...(prop.allOf ?? []).map((p) => p.$ref), ...(prop.oneOf ?? []).map((p) => p.$ref)]
      .filter((r): r is string => !!r)
      .map(strip);
    const fromRefs = refs.flatMap((r) => strings(this.schemas[r]?.enum));
    if (fromRefs.length) return fromRefs;
    return prop.items ? this.enumChoices(prop.items) : [];
  }

  plugins(): string[] {
    return [...new Set(this.types.map((t) => t.plugin).filter((p) => p !== "core"))].sort();
  }

  topLevel(): ResourceType[] {
    return this.types.filter((t) => !t.parent);
  }

  /** Creatable type of `category` for `plugin`, preferring the one named like the plugin. */
  pluginType(category: string, plugin: string, needCreate = true): ResourceType | undefined {
    const candidates = this.types.filter(
      (t) => t.category === category && t.plugin === plugin && !t.parent && (!needCreate || t.ops.create),
    );
    return candidates.find((t) => t.name === plugin) ?? candidates[0];
  }

  /** Content types of `plugin` creatable from an upload. */
  uploadTypes(plugin?: string): ResourceType[] {
    return this.types.filter(
      (t) =>
        t.category === "content" &&
        (!plugin || t.plugin === plugin) &&
        t.ops.create &&
        !!this.schema(t.create_schema)?.properties?.upload,
    );
  }
}

/** Django model of a type (`file.FileRepositoryResponse` → `file.filerepository`), for permissions. */
export function modelOf(t: ResourceType | undefined): string | null {
  const s = t?.read_schema;
  return s && s.endsWith("Response") ? s.slice(0, -"Response".length).toLowerCase() : null;
}
