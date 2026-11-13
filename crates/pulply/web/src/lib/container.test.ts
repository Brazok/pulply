import {describe, expect, it} from "vitest";

import {imageName, originHost, PULL_THROUGH_LABEL, pullThroughPath, registryRef, upstreamImage} from "./container";

describe("container registry paths", () => {
  it("takes the pull-through path from its label, not the UUID base path", () => {
    expect(pullThroughPath({base_path: "9190c809-db63-4d76-a610-01b0a24275d7", pulp_labels: {[PULL_THROUGH_LABEL]: "docker"}})).toBe("docker");
  });

  it("falls back to the base path on older pulp_container", () => {
    expect(pullThroughPath({base_path: "docker-cache", pulp_labels: {}})).toBe("docker-cache");
    expect(pullThroughPath({})).toBeNull();
  });

  it("names cached images relative to the prefix", () => {
    expect(imageName("docker/library/alpine", "docker")).toBe("library/alpine");
    expect(imageName("dockerx/alpine", "docker")).toBe("dockerx/alpine");
    expect(imageName("other/alpine", null)).toBe("other/alpine");
  });

  it("reduces typed references to the upstream image", () => {
    expect(upstreamImage(" docker.io/sbx/claude:latest ")).toBe("sbx/claude:latest");
    expect(upstreamImage("https://ghcr.io/org/app")).toBe("org/app");
    expect(upstreamImage("/library/alpine")).toBe("library/alpine");
  });
});

describe("registry references", () => {
  it("uses the content origin host rather than Pulp's registry_path", () => {
    const dist = {base_path: "busybox", registry_path: "localhost:18080/busybox"};
    expect(registryRef(dist, "10.0.0.171:18080")).toBe("10.0.0.171:18080/busybox");
    expect(registryRef(dist, undefined)).toBe("localhost:18080/busybox");
    expect(originHost("http://10.0.0.171:18080")).toBe("10.0.0.171:18080");
    expect(originHost("not a url")).toBeUndefined();
  });
});
