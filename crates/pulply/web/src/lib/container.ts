// Registry paths of container distributions, as `docker pull` / `podman pull` use them.

import type {Json} from "./api";

/**
 * Label where pulp_container (2.30+) keeps the registry path of a pull-through distribution:
 * its `base_path` is then a random UUID that clients must not use.
 */
export const PULL_THROUGH_LABEL = "pulp_container.pull_through";

const str = (v: unknown) => (typeof v === "string" && v ? v : null);

/** Path clients pull a pull-through distribution under (`docker` for `host/docker/<image>`). */
export function pullThroughPath(dist: Json): string | null {
  const labels = dist.pulp_labels as Record<string, unknown> | undefined;
  return str(labels?.[PULL_THROUGH_LABEL]) ?? str(dist.base_path);
}

/**
 * Image path relative to the pull-through prefix: `docker/library/alpine` → `library/alpine`.
 * Unchanged when it is not under the prefix.
 */
export function imageName(path: string, prefix: string | null): string {
  return prefix && path.startsWith(`${prefix}/`) ? path.slice(prefix.length + 1) : path;
}

/**
 * Image reference typed by a user, reduced to its upstream path: drops a scheme, a known
 * public registry host and a leading slash (`docker.io/library/alpine` → `library/alpine`).
 */
export function upstreamImage(input: string): string {
  return input
    .trim()
    .replace(/^[a-z]+:\/\//, "")
    .replace(/^(docker\.io|index\.docker\.io|registry-1\.docker\.io|quay\.io|ghcr\.io|gcr\.io|registry\.k8s\.io)\//, "")
    .replace(/^\/+/, "");
}


/**
 * Reference clients pull a container distribution with: the content origin's host and the
 * base path. Pulp's own `registry_path` uses the host its API was called with, which is
 * pulply's view of the server (often `localhost`), not the clients'.
 */
export function registryRef(dist: Json, host: string | undefined): string | null {
  const basePath = str(dist.base_path);
  if (host && basePath) return `${host}/${basePath}`;
  return str(dist.registry_path);
}

/** `host:port` of a content origin URL. */
export function originHost(origin: string | undefined): string | undefined {
  if (!origin) return undefined;
  try {
    return new URL(origin).host;
  } catch {
    return undefined;
  }
}
