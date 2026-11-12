import {Navigate, useSearchParams} from "react-router";

import {ResourceDetail} from "../components/detail/ResourceDetail";
import {TaskDetail} from "../components/detail/TaskDetail";
import {hrefPath} from "../lib/api";
import {base, listUrl} from "../lib/routes";
import {useSession} from "../lib/session";

/** `/item?href=<pulp href>`: a task, a list URL, or any resource. */
export function ItemPage() {
  const [params] = useSearchParams();
  const {registry, ctx} = useSession();
  const raw = params.get("href");
  if (!raw) return <Navigate replace to={base(ctx)} />;
  const href = hrefPath(raw);
  const type = registry.typeForHref(href);
  // An href to a list (`…/versions/`, `…/content/?repository_version=`) opens the list.
  const [path, query] = href.split("?", 2);
  if (type && (type.list_path === path || query)) {
    return <Navigate replace to={listUrl(ctx, type.id, {...Object.fromEntries(new URLSearchParams(query)), ...(type.list_path === path ? {} : {path})})} />;
  }
  if (type?.id === "tasks") return <TaskDetail key={href} href={href} />;
  return <ResourceDetail key={href} href={href} type={type} />;
}
