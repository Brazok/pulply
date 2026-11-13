import {describe, expect, it} from "vitest";

import {type FieldDef, buildBody} from "../components/SchemaForm";
import {hrefPath} from "./api";
import {fieldLabel, taskName} from "./format";
import {adminSection, buildNav, simplifyNav} from "./nav";
import {hrefOfPath, itemUrl, queryString} from "./routes";
import {scanRatio} from "./storage";
import {Registry, type ResourceType, versionNumber, versionRepository} from "./registry";

function type(id: string, list_path: string | null, extra: Partial<ResourceType> = {}): ResourceType {
  const [category, plugin, name] = id.split("/");
  return {
    id,
    category,
    plugin: plugin ?? "core",
    name: name ?? category,
    list_path,
    parent: null,
    generic: false,
    ops: {list: true, create: true, read: true, update: true, partial_update: true, delete: true},
    actions: [],
    children: [],
    list_params: [],
    read_schema: null,
    create_schema: null,
    create_multipart: false,
    update_schema: null,
    ...extra,
  };
}

const repo = type("repositories/file/file", "/pulp/api/v3/repositories/file/file/", {
  children: ["repositories/file/file/versions"],
  read_schema: "file.FileRepositoryResponse",
});
const versions = type("repositories/file/file/versions", null, {name: "versions", parent: ["repositories/file/file", "versions/"]});
const generic = type("repositories", "/pulp/api/v3/repositories/", {generic: true});
const reg = new Registry({
  types: [repo, versions, generic, type("tasks", "/pulp/api/v3/tasks/")],
  schemas: {PolicyEnum: {enum: ["immediate", "on_demand"]}},
  operations: {},
});

describe("registry", () => {
  it("resolves hrefs, versions and PRNs to their type", () => {
    const href = "/pulp/api/v3/repositories/file/file/0192/";
    expect(reg.typeForHref(href)?.id).toBe("repositories/file/file");
    expect(reg.typeForHref(`${href}versions/3/`)?.id).toBe("repositories/file/file/versions");
    expect(reg.typeForHref("/pulp/api/v3/tasks/01/")?.id).toBe("tasks");
    expect(reg.typeForPrn("prn:file.filerepository:0192")?.id).toBe("repositories/file/file");
    expect(versionNumber(`${href}versions/12/`)).toBe(12);
    expect(versionRepository(`${href}versions/12/`)).toBe(href);
  });

  it("reads enum choices through references", () => {
    expect(reg.enumChoices({allOf: [{$ref: "#/components/schemas/PolicyEnum"}]})).toEqual(["immediate", "on_demand"]);
    expect(reg.enumChoices({type: "array", items: {enum: ["a", null, "b"]}})).toEqual(["a", "b"]);
  });

  it("turns pagination URLs into proxy paths", () => {
    expect(hrefPath("http://pulp:8080/pulp/api/v3/tasks/?offset=25")).toBe("/pulp/api/v3/tasks/?offset=25");
    expect(hrefPath("/pulp/api/v3/tasks/")).toBe("/pulp/api/v3/tasks/");
  });
});

const field = (name: string, k: FieldDef["kind"], extra: Partial<FieldDef> = {}): FieldDef => ({
  name,
  kind: k,
  required: false,
  nullable: true,
  help: null,
  placeholder: null,
  ...extra,
});

describe("form bodies", () => {
  const fields = [
    field("name", {k: "text"}, {required: true}),
    field("retain_repo_versions", {k: "integer"}),
    field("description", {k: "text"}),
    field("pulp_labels", {k: "json"}),
    field("file", {k: "file"}),
  ];

  it("sends only changed fields on edit, null for cleared nullable ones", () => {
    const original = {name: "iso", retain_repo_versions: 3, description: "old", pulp_labels: {a: "b"}};
    const values = {name: "iso", retain_repo_versions: "5", description: "", pulp_labels: '{"a": "b"}'};
    expect(buildBody(fields, {values, files: {}}, original)).toEqual({
      value: {retain_repo_versions: 5, description: null},
      files: {},
    });
  });

  it("requires required fields on creation and types values", () => {
    expect(buildBody(fields, {values: {name: ""}, files: {}}, null)).toEqual({errors: {name: ["Required"]}});
    const file = new File(["x"], "x.txt");
    expect(buildBody(fields, {values: {name: "iso", retain_repo_versions: "2"}, files: {file}}, null)).toEqual({
      value: {name: "iso", retain_repo_versions: 2},
      files: {file},
    });
    expect(buildBody(fields, {values: {name: "iso", pulp_labels: "{"}, files: {}}, null)).toHaveProperty("errors.pulp_labels");
  });
});

describe("labels", () => {
  it("names tasks by their verb", () => {
    expect(taskName("pulpcore.app.tasks.base.general_update")).toBe("Update");
    expect(taskName("pulpcore.app.tasks.base.ageneral_create")).toBe("Create");
    expect(taskName("pulpcore.app.tasks.orphan.orphan_cleanup")).toBe("Orphan cleanup");
  });
  it("drops the href suffix of field labels", () => {
    expect(fieldLabel("latest_version_href")).toBe("Latest version");
    expect(fieldLabel("pulp_href")).toBe("Href");
  });
});

describe("administration navigation", () => {
  const admin = new Registry({
    types: [repo, generic, ...["tasks", "users", "groups", "workers", "artifacts", "frobnicators"].map((id) => type(id, `/pulp/api/v3/${id}/`))],
    schemas: {},
    operations: {},
  });

  it("groups core types by section, with maintenance under Server and unknown types under Other", () => {
    const nav = buildNav(admin, "lab");
    expect(nav.overview.map((i) => i.id)).toEqual(["dashboard", "tasks"]);
    expect(nav.admin.map((s) => [s.id, s.items.map((i) => i.id)])).toEqual([
      ["access", ["users", "groups"]],
      ["server", ["workers", "operations"]],
      ["storage", ["storage", "artifacts"]],
      ["other", ["frobnicators"]],
    ]);
    expect(adminSection(admin, "lab", "groups")?.id).toBe("access");
    expect(adminSection(admin, "lab", "repositories/file/file")).toBeUndefined();
  });

  it("is hidden in simple mode", () => {
    expect(simplifyNav(buildNav(admin, "lab")).admin).toEqual([]);
  });
});


describe("readable URLs", () => {
  const href = "/pulp/api/v3/repositories/file/file/0192abcd-0000-7000-8000-000000000001/";
  it("addresses a resource by its href without the API prefix", () => {
    expect(itemUrl("lab", href)).toBe("/c/lab/repositories/file/file/0192abcd-0000-7000-8000-000000000001/");
    expect(itemUrl("lab", `http://pulp:8080${href}versions/2/`)).toBe("/c/lab/repositories/file/file/0192abcd-0000-7000-8000-000000000001/versions/2/");
    expect(itemUrl("lab", "/pulp/acme/api/v3/tasks/01/")).toBe("/c/lab/tasks/01/");
    expect(hrefOfPath("/repositories/file/file/0192abcd-0000-7000-8000-000000000001/", "/pulp/api/v3/")).toBe(href);
    expect(hrefOfPath("/storage", "/pulp/api/v3/")).toBeNull();
    expect(hrefOfPath("", "/pulp/api/v3/")).toBeNull();
  });

  it("keeps hrefs legible in query strings", () => {
    expect(queryString({repository_version: `${href}versions/2/`, state__in: "running,waiting"})).toBe(
      `?repository_version=${href}versions/2/&state__in=running,waiting`,
    );
    expect(queryString({q: "a b&c"})).toBe("?q=a%20b%26c");
    expect(queryString({})).toBe("");
  });
});

describe("sidebar", () => {
  const sidebar = new Registry({
    types: [
      repo,
      generic,
      type("remotes/file/file", "/pulp/api/v3/remotes/file/file/"),
      type("remotes/file/git", "/pulp/api/v3/remotes/file/git/"),
      type("remotes/container/container", "/pulp/api/v3/remotes/container/container/"),
      type("remotes/container/pull-through", "/pulp/api/v3/remotes/container/pull-through/"),
      type("repositories/core/openpgp-keyring", "/pulp/api/v3/repositories/core/openpgp-keyring/"),
    ],
    schemas: {},
    operations: {},
  });

  it("lists cross-plugin lists by category and core types as a plugin group", () => {
    const nav = buildNav(sidebar, "lab");
    expect(nav.platform.map((i) => [i.label, i.category])).toEqual([["Repositories", "repositories"]]);
    expect(nav.plugins.map((g) => g.id)).toEqual(["container", "file", "core"]);
    expect(simplifyNav(nav).plugins.map((g) => g.id)).toEqual(["container", "file"]);
  });

  it("offers the variants of a type to pickers", () => {
    expect(sidebar.variants(sidebar.get("remotes/file/file")!).map((t) => t.id)).toEqual(["remotes/file/file", "remotes/file/git"]);
    expect(sidebar.variants(generic).map((t) => t.id)).toEqual(["repositories"]);
  });
});

describe("disk usage scan", () => {
  it("reports the overall progress over its steps", () => {
    expect(scanRatio(null)).toBeNull();
    expect(scanRatio({step: 1, steps: 5, label: "", done: 0, total: 100})).toBe(0);
    expect(scanRatio({step: 1, steps: 5, label: "", done: 50, total: 100})).toBeCloseTo(0.1);
    expect(scanRatio({step: 3, steps: 5, label: "", done: 0, total: 0})).toBeCloseTo(0.4);
    expect(scanRatio({step: 5, steps: 5, label: "", done: 10, total: 10})).toBe(1);
  });
});
