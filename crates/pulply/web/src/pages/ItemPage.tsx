import {Navigate, useLocation, useParams, useSearchParams} from "react-router";

import {ResourceDetail} from "../components/detail/ResourceDetail";
import {TaskDetail} from "../components/detail/TaskDetail";
import {hrefPath} from "../lib/api";
import {base, hrefOfPath, itemUrl, listUrl} from "../lib/routes";
import {useSession} from "../lib/session";

/**
 * `/c/<ctx>/<href without the API prefix>`: a task, a list URL, or any resource.
 * The former `/item?href=<pulp href>` redirects there.
 */
export function ItemPage() {
  const [params] = useSearchParams();
  const rest = useParams()["*"] ?? "";
  const location = useLocation();
  const {registry, ctx, info} = useSession();
  const legacy = params.get("href");
  if (rest === "item" || rest === "item/") {
    return <Navigate replace to={legacy ? itemUrl(ctx, legacy) : base(ctx)} />;
  }
  const fromPath = hrefOfPath(rest, info.api_prefix);
  if (!fromPath) return <Navigate replace to={base(ctx)} />;
  const href = hrefPath(fromPath + location.search);
  const type = registry.typeForHref(href);
  // An href to a list (`…/versions/`, `…/content/?repository_version=`) opens the list.
  const [path, query] = href.split("?", 2);
  if (type && (type.list_path === path || query)) {
    return <Navigate replace to={listUrl(ctx, type.id, {...Object.fromEntries(new URLSearchParams(query)), ...(type.list_path === path ? {} : {path})})} />;
  }
  if (type?.id === "tasks") return <TaskDetail key={href} href={href} />;
  return <ResourceDetail key={href} href={href} type={type} />;
}
