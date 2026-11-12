import {describe, expect, it} from "vitest";

import {type FieldDef, buildBody} from "../components/SchemaForm";
import {hrefPath} from "./api";
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
