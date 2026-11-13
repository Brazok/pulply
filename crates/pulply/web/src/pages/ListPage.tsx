import {useCallback, useState} from "react";
import {Lightbulb} from "lucide-react";
import {useLocation, useNavigate, useParams, useSearchParams} from "react-router";

import {AddContentHelp, hasAddContentHelp} from "../components/AddContentHelp";
import {ResourceList} from "../components/ResourceList";
import {Button} from "../components/ui/button";
import {PageHeader} from "../components/PageHeader";
import {ErrorScreen} from "../components/screens";
import {Badge} from "../components/ui/badge";
import {pluginLabel, typeTitle} from "../lib/format";
import {ADMIN_DESCRIPTIONS, adminSection} from "../lib/nav";
import {adminUrl, base, queryString} from "../lib/routes";
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
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const location = useLocation();
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
      const next: Record<string, string> = {...filters};
      const p = params.get("path");
      if (p) next.path = p;
      const search = queryString(next);
      if (search !== location.search) navigate({search}, {replace: true});
    },
    [params, navigate, location.search],
  );
  const admin = type && adminSection(registry, ctx, type.id);
  const [help, setHelp] = useState(false);
  const showHelp = !!type && ["repositories", "content"].includes(type.category) && hasAddContentHelp(registry, type.plugin);
  if (!type || !path) return <ErrorScreen error={new Error(`Unknown resource type "${typeId}".`)} />;
  return (
    <>
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
            crumbs={[
              {label: "Home", to: base(ctx)},
              ...(admin ? [{label: "Administration", to: adminUrl(ctx)}] : []),
              {label: typeTitle(type)},
            ]}
            subtitle={DESCRIPTIONS[type.category] ?? ADMIN_DESCRIPTIONS[type.id]}
            title={typeTitle(type)}
            actions={
              showHelp && (
                <Button variant="outline" onClick={() => setHelp(true)}>
                  <Lightbulb />
                  How to add content
                </Button>
              )
            }
          />
        }
        type={type}
        onFiltersChange={onFiltersChange}
      />
      {help && <AddContentHelp plugin={type.plugin} onClose={() => setHelp(false)} />}
    </>
  );
}
