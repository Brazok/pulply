// Table columns per resource type: presets for core and plugin types, completed from
// the read schema (same rules as the terminal UI).

import {Chip} from "@heroui/react";
import type {ReactNode} from "react";

import {HrefLink, PrnLink, StateChip, TimeAgo, Value} from "../components/values";
import type {Json} from "./api";
import {bytes, contentSummary, duration, fieldLabel, nevra, scalarText, taskName, typeTitle} from "./format";
import type {Registry, ResourceType, Schema} from "./registry";

export interface Column {
  id: string;
  header: string;
  /** API fields needed (for `?fields=` and schema filtering). */
  fields: string[];
  /** Field usable with `?ordering=`. */
  sort?: string;
  render: (item: Json) => ReactNode;
  /** Main column: the row's link target. */
  primary?: boolean;
}

const str = (v: unknown) => (typeof v === "string" ? v : "");

function field(header: string, f: string, primary = false): Column {
  return {id: f, header, fields: [f], sort: f, render: (i) => <Value name={f} value={i[f]} />, primary};
}

function text(header: string, f: string, primary = false): Column {
  return {
    id: f,
    header,
    fields: [f],
    sort: f,
    primary,
    render: (i) => <span className={primary ? "font-medium" : ""}>{scalarText(i[f]) || "—"}</span>,
  };
}

function href(header: string, f: string): Column {
  return {id: f, header, fields: [f], render: (i) => (str(i[f]) ? <HrefLink href={str(i[f])} /> : <Muted />)};
}

function anyHref(header: string, fs: string[]): Column {
  return {
    id: fs.join("|"),
    header,
    fields: fs,
    render: (i) => {
      const h = fs.map((f) => str(i[f])).find(Boolean);
      return h ? <HrefLink href={h} /> : <Muted />;
    },
  };
}

function age(header: string, f: string): Column {
  return {id: f, header, fields: [f], sort: f, render: (i) => <TimeAgo ts={str(i[f])} />};
}

function size(f: string): Column {
  return {id: f, header: "Size", fields: [f], sort: f, render: (i) => <span>{bytes(i[f] as number) || "—"}</span>};
}

function join(header: string, fs: string[], sep: string, primary = false): Column {
  return {
    id: fs.join("+"),
    header,
    fields: fs,
    sort: fs[0],
    primary,
    render: (i) => (
      <span className={primary ? "font-medium" : ""}>
        {fs
          .map((f) => scalarText(i[f]))
          .filter(Boolean)
          .join(sep)}
      </span>
    ),
  };
}

function Muted() {
  return <span className="text-muted">—</span>;
}

const versionOf = (f: string): Column => ({
  id: f,
  header: "Latest version",
  fields: [f],
  render: (i) => (str(i[f]) ? <HrefLink href={str(i[f])} label={`v${str(i[f]).replace(/\/+$/, "").split("/").at(-1)}`} /> : <Muted />),
});

const summary: Column = {
  id: "content_summary",
  header: "Content",
  fields: ["content_summary"],
  render: (i) => {
    const s = contentSummary(i.content_summary);
    return (
      <span className="font-mono text-xs">
        <span className="text-success">+{s.added}</span> <span className="text-danger">−{s.removed}</span>{" "}
        <span className="text-muted">={s.present}</span>
      </span>
    );
  },
};

const TASK_COLUMNS: Column[] = [
  {id: "state", header: "State", fields: ["state"], sort: "state", render: (i) => <StateChip state={i.state} />},
  {
    id: "name",
    header: "Task",
    fields: ["name"],
    sort: "name",
    primary: true,
    render: (i) => (
      <span className="font-medium" title={str(i.name)}>
        {taskName(i.name)}
      </span>
    ),
  },
  {
    id: "resources",
    header: "Resources",
    fields: ["reserved_resources_record"],
    render: (i) => {
      const recs = ((i.reserved_resources_record as string[]) ?? []).filter(
        (r) => !r.startsWith("shared:") && !r.startsWith("prn:core.domain:"),
      );
      const first = recs.find((r) => r.startsWith("/") || r.startsWith("prn:"));
      if (!first) return <Muted />;
      return first.startsWith("/") ? <HrefLink href={first} /> : <PrnLink prn={first} />;
    },
  },
  {
    id: "duration",
    header: "Duration",
    fields: ["started_at", "finished_at"],
    sort: "started_at",
    render: (i) => <span className="whitespace-nowrap">{duration(str(i.started_at), str(i.finished_at) || null) || "—"}</span>,
  },
  age("Created", "pulp_created"),
];

function pluginColumns(t: ResourceType): Column[] | null {
  const c = t.category;
  const n = t.name;
  switch (t.plugin) {
    case "rpm":
      if (c !== "content") return null;
      if (n === "packages") {
        return [
          {id: "nevra", header: "Package", fields: ["name", "epoch", "version", "release", "arch"], sort: "name", primary: true, render: (i) => <span className="font-medium">{nevra(i)}</span>},
          size("size_package"),
          text("Summary", "summary"),
        ];
      }
      if (n === "advisories") return [text("ID", "id", true), text("Type", "type"), text("Severity", "severity"), text("Issued", "issued_date"), text("Title", "title")];
      if (n === "modulemds") return [text("Name", "name", true), text("Stream", "stream"), text("Version", "version"), text("Context", "context"), text("Arch", "arch")];
      if (n === "distribution_trees") return [join("Release", ["release_name", "release_version"], " ", true), text("Arch", "arch")];
      if (["packagegroups", "packageenvironments", "packagecategories"].includes(n)) return [text("ID", "id", true), text("Name", "name"), field("Packages", "packages")];
      return null;
    case "container":
      if (c === "content" && n === "tags") return [text("Tag", "name", true), href("Manifest", "tagged_manifest")];
      if (c === "content" && n === "manifests") return [text("Digest", "digest", true), text("Media type", "media_type"), text("Arch", "architecture"), text("OS", "os"), size("compressed_image_size")];
      if (c === "content" && n === "blobs") return [text("Digest", "digest", true)];
      if (c === "distributions") return [text("Name", "name", true), text("Registry path", "registry_path"), anyHref("Serves", ["repository", "repository_version"]), field("Private", "private")];
      return null;
    case "file":
      if (c === "content" && n === "files") return [text("Relative path", "relative_path", true), text("SHA-256", "sha256")];
      if (c === "publications") return [href("Version", "repository_version"), text("Manifest", "manifest")];
      return null;
    case "python":
      if (c === "content" && n === "packages") return [text("Name", "name", true), text("Version", "version"), text("Type", "packagetype"), text("Filename", "filename"), size("size")];
      return null;
    case "ansible":
      if (c === "content" && n === "collection_versions") return [join("Collection", ["namespace", "name"], ".", true), text("Version", "version"), text("Requires Ansible", "requires_ansible")];
      if (c === "content" && n === "roles") return [join("Role", ["namespace", "name"], ".", true), text("Version", "version")];
      if (c === "distributions") return [text("Name", "name", true), text("Base path", "base_path"), text("Client URL", "client_url")];
      return null;
    case "deb":
      if (c !== "content") return null;
      if (n === "releases") return [text("Codename", "codename", true), text("Suite", "suite"), text("Distribution", "distribution"), text("Version", "version")];
      if (n === "release_components") return [text("Component", "component", true), text("Distribution", "distribution")];
      if (n === "release_architectures") return [text("Architecture", "architecture", true), text("Distribution", "distribution")];
      if (n === "packages") return [text("Package", "package", true), text("Version", "version"), text("Architecture", "architecture"), text("Section", "section"), text("Relative path", "relative_path")];
      return null;
    default:
      return null;
  }
}

function preset(t: ResourceType): Column[] {
  const c = t.category;
  if (t.parent && t.name === "roles") return [text("Role", "role", true), href("Object", "content_object")];
  if (c === "users" || t.name === "users") {
    return [text("Username", "username", true), text("First name", "first_name"), text("Last name", "last_name"), text("Email", "email"), field("Staff", "is_staff"), field("Active", "is_active")];
  }
  if (t.name === "versions" || c === "repository_versions") return [text("Version", "number", true), summary, href("Base", "base_version"), href("Repository", "repository")];
  switch (c) {
    case "roles":
      return [text("Name", "name", true), field("Locked", "locked"), text("Description", "description")];
    case "access_policies":
      return [text("Viewset", "viewset_name", true), field("Customized", "customized")];
    case "upstream-pulps":
      return [text("Name", "name", true), text("URL", "base_url"), age("Last replication", "last_replication")];
    case "task-schedules":
      return [text("Name", "name", true), text("Task", "task_name"), text("Interval", "dispatch_interval"), age("Next", "next_dispatch")];
    case "signing-services":
      return [text("Name", "name", true), text("Fingerprint", "pubkey_fingerprint")];
    case "repositories":
      return [text("Name", "name", true), versionOf("latest_version_href"), href("Remote", "remote"), text("Retained versions", "retain_repo_versions")];
    case "remotes":
      return [text("Name", "name", true), field("URL", "url"), field("Policy", "policy")];
    case "distributions":
      return [text("Name", "name", true), text("Base path", "base_path"), anyHref("Serves", ["repository", "publication", "repository_version"]), field("Base URL", "base_url")];
    case "publications":
      return [href("Version", "repository_version"), href("Repository", "repository")];
    case "workers":
      return [text("Name", "name", true), age("Last heartbeat", "last_heartbeat"), href("Current task", "current_task")];
    case "artifacts":
      return [text("SHA-256", "sha256", true), size("size"), text("File", "file")];
    case "uploads":
      return [size("size"), age("Completed", "completed")];
    case "task-groups":
      return [text("Description", "description", true), field("All tasks dispatched", "all_tasks_dispatched")];
    default:
      return [];
  }
}

const SKIP = new Set([
  "pulp_href", "prn", "pulp_created", "pulp_last_updated", "pulp_labels", "description", "md5", "sha1", "sha224",
  "sha384", "sha512", "artifact", "artifacts", "vuln_report", "hidden_fields", "headers", "ca_cert", "client_cert",
  "client_key", "password", "username", "proxy_url", "proxy_username", "proxy_password", "versions_href",
  "content_guard", "pulp_domain", "pkgId", "checksum_type", "location_base", "location_href", "summary", "url",
]);

const MAX = 6;

/** Type of each row in cross-plugin lists (`/content/`, `/repositories/`). */
function typeColumn(reg: Registry): Column {
  return {
    id: "__type",
    header: "Type",
    fields: ["pulp_href"],
    render: (i) => {
      const t = reg.typeForHref(str(i.pulp_href));
      return t ? <Chip size="sm" variant="soft">{typeTitle(t)}</Chip> : <Muted />;
    },
  };
}

export function columnsFor(t: ResourceType, schema: Schema | undefined, reg?: Registry): Column[] {
  if (t.category === "tasks") return TASK_COLUMNS;
  const props = schema?.properties ?? {};
  const has = (f: string) => !schema || f in props;
  let cols = (pluginColumns(t) ?? preset(t)).filter((c) => c.fields.some(has));
  if (schema && cols.length < MAX - 1) {
    if (!cols.length && "name" in props) cols.push(text("Name", "name", true));
    for (const [name, p] of Object.entries(props)) {
      if (cols.length >= MAX - 1) break;
      const scalar = ["string", "integer", "number", "boolean"].includes(p.type ?? "") || !!p.allOf || !!p.oneOf;
      if (cols.some((c) => c.fields.includes(name)) || SKIP.has(name) || name.endsWith("_prn") || p.writeOnly || !scalar) continue;
      cols.push(p.format === "uri" ? href(fieldLabel(name), name) : p.format === "date-time" ? age(fieldLabel(name), name) : field(fieldLabel(name), name));
    }
  }
  if (!cols.length) cols.push(href("Resource", "pulp_href"));
  if (!cols.some((c) => c.primary)) cols = [{...cols[0], primary: true}, ...cols.slice(1)];
  if (t.generic && reg) cols.splice(1, 0, typeColumn(reg));
  if (has("pulp_created") && !cols.some((c) => c.id === "pulp_created")) cols.push(age("Created", "pulp_created"));
  return cols;
}

/** Copy-paste commands to consume a distribution. */
export function usageLines(t: ResourceType, item: Json): string[] {
  if (t.category !== "distributions") return [];
  const raw = str(item.base_url);
  const url = raw && !raw.endsWith("/") ? `${raw}/` : raw;
  const name = str(item.name) || "pulp";
  switch (t.plugin) {
    case "rpm":
      return url ? [`dnf config-manager --add-repo ${url}`, `[${name}]\nbaseurl=${url}\ngpgcheck=0`] : [];
    case "container":
      return item.registry_path ? [`podman pull ${str(item.registry_path)}:<tag>`] : [];
    case "file":
      return url ? [`curl ${url}PULP_MANIFEST`] : [];
    case "python":
      return url
        ? [`pip install --index-url ${url}simple/ <package>`, ...(item.allow_uploads ? [`twine upload --repository-url ${url}legacy/ dist/*`] : [])]
        : [];
    case "ansible":
      return item.client_url ? [`ansible-galaxy collection install <namespace.name> -s ${str(item.client_url)}`] : [];
    case "deb":
      return url ? [`deb ${url} <distribution> <component>`] : [];
    case "npm":
      return url ? [`npm install --registry ${url} <package>`] : [];
    case "maven":
      return url ? [`<repository><url>${url}</url></repository>`] : [];
    default:
      return url ? [url] : [];
  }
}
