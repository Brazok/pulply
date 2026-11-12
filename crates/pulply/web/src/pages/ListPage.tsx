import {useCallback} from "react";
import {useParams, useSearchParams} from "react-router";

import {ResourceList} from "../components/ResourceList";
import {PageHeader} from "../components/PageHeader";
import {ErrorScreen} from "../components/screens";
import {Badge} from "../components/ui/badge";
import {pluginLabel, typeTitle} from "../lib/format";
import {base} from "../lib/routes";
import {useSession} from "../lib/session";

const DESCRIPTIONS: Record<string, string> = {
  repositories: "Versioned collections of content. Sync them from a remote, or upload content.",
  remotes: "Where content is synced from.",
  distributions: "Serve a repository, version or publication to clients.",
  publications: "Repository versions prepared for distribution.",
  content: "Content units stored in Pulp.",
  tasks: "Background work of the server: syncs, publications, uploads…",
  workers: "Processes running tasks.",
};

/** `/browse/<type id>?<filters>`; `path=` lists a nested collection or a filtered URL. */
export function ListPage() {
  const typeId = useParams()["*"] ?? "";
  const [params, setParams] = useSearchParams();
  const {registry, ctx} = useSession();
  const type = registry.get(typeId.replace(/\/+$/, ""));
  const path = params.get("path") ?? type?.list_path ?? null;
  // Read from the URL when the list mounts; the list then owns its filters and mirrors them back.
  const initialFilters: Record<string, string> = {};
  params.forEach((v, k) => {
    if (k !== "path") initialFilters[k] = v;
  });
  const onFiltersChange = useCallback(
    (filters: Record<string, string>) => {
      const next = new URLSearchParams(filters);
      const p = params.get("path");
      if (p) next.set("path", p);
      if (next.toString() !== params.toString()) setParams(next, {replace: true});
    },
    [params, setParams],
  );
  if (!type || !path) return <ErrorScreen error={new Error(`Unknown resource type "${typeId}".`)} />;
  return (
    <ResourceList
      key={`${type.id}|${path}`}
      initialFilters={initialFilters}
      path={path}
      title={
        <PageHeader
          chips={
            type.plugin !== "core" && (
<Badge variant="secondary">{pluginLabel(type.plugin)}</Badge>
            )
          }
          crumbs={[{label: "Home", to: base(ctx)}, {label: typeTitle(type)}]}
          subtitle={DESCRIPTIONS[type.category]}
          title={typeTitle(type)}
        />
      }
      type={type}
      onFiltersChange={onFiltersChange}
    />
  );
}
