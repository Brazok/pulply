import {describe, expect, it} from "vitest";

import {buildGraph, connected} from "./graph";

const repo = "/pulp/api/v3/repositories/file/file/r1/";
const remote = "/pulp/api/v3/remotes/file/file/m1/";
const v1 = `${repo}versions/1/`;
const v3 = `${repo}versions/3/`;
const pub = "/pulp/api/v3/publications/file/file/p1/";
const distPub = "/pulp/api/v3/distributions/file/file/d1/";
const distRepo = "/pulp/api/v3/distributions/file/file/d2/";

const graph = buildGraph({
  repositories: [{pulp_href: repo, name: "iso", remote, latest_version_href: v3}],
  remotes: [{pulp_href: remote, name: "upstream", url: "https://example.org/"}],
  versions: [{pulp_href: v3, number: 3, content_summary: {present: {"file.file": {count: 2}}}}],
  publications: [{pulp_href: pub, repository_version: v1, repository: repo}],
  distributions: [
    {pulp_href: distPub, name: "pinned", base_path: "pinned", publication: pub, base_url: "http://pulp/content/pinned/"},
    {pulp_href: distRepo, name: "live", base_path: "live", repository: repo, base_url: "http://pulp/content/live/"},
  ],
});
const edge = (from: string, to: string) => graph.edges.some((e) => e.from === from && e.to === to);

describe("relation graph", () => {
  it("follows content from the remote to the clients", () => {
    expect(edge(remote, repo)).toBe(true);
    expect(edge(repo, v3)).toBe(true);
    expect(edge(v1, pub)).toBe(true);
    expect(edge(pub, distPub)).toBe(true);
    expect(graph.edges.find((e) => e.from === repo && e.to === distRepo)?.label).toBe("serves the latest version");
    expect(edge(distPub, "http://pulp/content/pinned/")).toBe(true);
  });
  it("adds the published versions and marks the latest one", () => {
    const versions = graph.nodes.filter((n) => n.kind === "version");
    expect(versions.map((n) => n.label).sort()).toEqual(["Version 1", "Version 3"]);
    expect(versions.find((n) => n.id === v3)).toMatchObject({latest: true, detail: "2 content units"});
  });
  it("highlights the path through a resource", () => {
    const path = connected(graph, pub);
    expect([...path].sort()).toEqual([remote, repo, v1, pub, distPub, "http://pulp/content/pinned/"].sort());
    expect(path.has(distRepo)).toBe(false);
  });
});
