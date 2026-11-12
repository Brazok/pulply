// Detail page of any resource: workflow actions, generic actions menu, tabs for the
// overview, content, nested collections, relations, roles and raw JSON.

import {useQueryClient} from "@tanstack/react-query";
import {Copy, Ellipsis, Globe, Pencil, RefreshCw, Rocket, Trash2, Undo2, CloudUpload} from "lucide-react";
import {type ReactNode, useState} from "react";
import {useNavigate} from "react-router";
import {toast} from "sonner";

import type {Json} from "../../lib/api";
import {usageLines} from "../../lib/columns";
import {displayName, fieldLabel, pluginLabel, singular, titleize, typeLabel, typeTitle} from "../../lib/format";
import {type Action, type ResourceType, versionNumber, versionRepository} from "../../lib/registry";
import {useMode} from "../../lib/mode";
import {base, itemUrl, listUrl} from "../../lib/routes";
import {errorText, usePulp, useSession} from "../../lib/session";
import {ConfirmDialog, ResultDialog} from "../dialogs";
import {type Crumb, PageHeader} from "../PageHeader";
import {ResourceList} from "../ResourceList";
import {type FormSpec, FormModal} from "../SchemaForm";
import {UploadDialog} from "../UploadDialog";
import {Badge} from "../ui/badge";
import {Button} from "../ui/button";
import {Card, CardAction, CardContent, CardHeader, CardTitle} from "../ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "../ui/dropdown-menu";
import {Spinner} from "../ui/spinner";
import {Tabs, TabsContent, TabsList, TabsTrigger} from "../ui/tabs";
import {Tooltip, TooltipContent, TooltipTrigger} from "../ui/tooltip";
import {JsonView, Value} from "../values";
import {ContentSummary} from "./ContentSummary";
import {RolesPanel} from "./RolesPanel";

/** Actions with their own button or view. */
const DEDICATED = new Set(["sync", "list_roles", "add_role", "remove_role", "my_permissions", "modify"]);
/** Fields shown in the header instead of the details grid. */
const HIDDEN = new Set(["pulp_href", "name"]);
/** Bookkeeping fields hidden in simple mode. */
const TECHNICAL = new Set(["prn", "pulp_last_updated", "versions_href", "pulp_domain", "hidden_fields", "description"]);

function isEmpty(v: unknown): boolean {
  if (v === null || v === undefined || v === "") return true;
  if (Array.isArray(v)) return v.length === 0;
  return typeof v === "object" && Object.keys(v as object).length === 0;
}

export function CopyButton({text, label = "Copy"}: {text: string; label?: string}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          aria-label={label}
          size="icon-xs"
          variant="ghost"
          onClick={async () => {
            await navigator.clipboard.writeText(text);
            toast.success("Copied to the clipboard", {duration: 1500});
          }}
        >
          <Copy />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

/** Label / value rows. Simple mode leaves out empty and bookkeeping fields. */
function Details({item, simple}: {item: Json; simple: boolean}) {
  const entries = Object.entries(item).filter(
    ([k, v]) => !HIDDEN.has(k) && !(simple && (TECHNICAL.has(k) || k.endsWith("_prn") || isEmpty(v))),
  );
  if (!entries.length) return <p className="text-sm text-muted-foreground">Nothing more to show.</p>;
  return (
    <dl className="grid grid-cols-1 gap-x-10 md:grid-cols-2">
      {entries.map(([k, v]) => (
        <div key={k} className="grid min-w-0 grid-cols-[minmax(7rem,11rem)_1fr] items-baseline gap-4 border-b py-2.5">
          <dt className="text-sm text-muted-foreground">{fieldLabel(k)}</dt>
          <dd className="min-w-0 text-sm">
            <Value name={k} value={v} />
          </dd>
        </div>
      ))}
    </dl>
  );
}

function Section({title, children, actions}: {title: string; children: ReactNode; actions?: ReactNode}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        {actions && <CardAction>{actions}</CardAction>}
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}

export function ResourceDetail({href, type}: {href: string; type: ResourceType | undefined}) {
  const {registry, pulp, ctx, info, can, track} = useSession();
  const {simple} = useMode();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const query = usePulp(href);
  const item = query.data;
  const hasPerms = !!type?.actions.some((a) => a.verb === "my_permissions");
  const objectPermsQuery = usePulp<{permissions: string[]}>(
    hasPerms && info.access !== "all" ? `${href}my_permissions/` : null,
  );
  const objectPerms = info.access === "all" ? null : (objectPermsQuery.data?.permissions ?? []);
  const allowed = (action: string) => can(action, type, objectPerms);
  const version = versionNumber(href);
  const repoOfVersion = version !== null ? versionRepository(href) : null;
  const owningRepo = usePulp(repoOfVersion, {fields: "pulp_href,name,latest_version_href"});

  const [form, setForm] = useState<FormSpec | null>(null);
  const [confirm, setConfirm] = useState<{title: string; body?: ReactNode; danger?: boolean; run: () => Promise<unknown>} | null>(null);
  const [result, setResult] = useState<{title: string; value: unknown} | null>(null);
  const [upload, setUpload] = useState(false);
  const [tab, setTab] = useState<string>("overview");

  if (query.isLoading) return <Spinner className="m-10 size-6" />;
  if (query.error || !item) {
    return <div className="rounded-lg bg-destructive/10 p-4 text-sm text-destructive">{errorText(query.error ?? "Not found")}</div>;
  }

  const plugin = type?.plugin ?? "core";
  const isLatest = owningRepo.data?.latest_version_href === href;
  const isRepository = type?.category === "repositories" && !type.parent;
  const name = version !== null ? `Version ${version}` : displayName(item, href);
  const parentType = repoOfVersion ? registry.typeForHref(repoOfVersion) : undefined;
  const repoForModify = isRepository ? href : repoOfVersion;
  const modifyType = repoForModify ? registry.typeForHref(repoForModify) : undefined;
  const canModify = !!modifyType?.actions.some((a) => a.verb === "modify") && can("modify", modifyType, objectPerms);

  // Workflows.
  const syncAction = type?.actions.find((a) => a.verb === "sync" && a.method === "POST");
  const pubType = isRepository || version !== null ? registry.pluginType("publications", plugin) : undefined;
  const distType =
    isRepository || version !== null || type?.category === "publications" ? registry.pluginType("distributions", plugin) : undefined;
  const uploadTypes = isRepository && canModify ? registry.uploadTypes(plugin) : [];

  const openAction = (a: Action) => {
    const schema = registry.schema(a.body_schema);
    const writable = Object.values(schema?.properties ?? {}).some((p) => !p.readOnly);
    const label = `${titleize(a.verb)} ${name}`;
    if (a.method === "GET") {
      pulp
        .get(`${href}${a.verb}/`)
        .then((value) => setResult({title: `${titleize(a.verb)} · ${name}`, value}))
        .catch((e) => toast.error(errorText(e)));
      return;
    }
    if (writable) {
      setForm({
        title: `${titleize(a.verb)} · ${name}`,
        label,
        method: "POST",
        path: `${href}${a.verb}/`,
        schema,
        plugin,
        multipart: a.multipart,
        showResult: true,
        initial: a.verb === "sync" ? {remote: item.remote} : undefined,
        submitLabel: titleize(a.verb),
      });
    } else {
      setConfirm({
        title: `${titleize(a.verb)} ${name}?`,
        run: async () => {
          const resp = await pulp.send("POST", `${href}${a.verb}/`, {value: {}, multipart: a.multipart});
          if (!track(resp, label)) setResult({title: label, value: resp});
          queryClient.invalidateQueries({queryKey: ["pulp", ctx]});
        },
      });
    }
  };

  const publish = () => {
    if (!pubType) return;
    setForm({
      title: `Publish ${name}`,
      label: `Publish ${name}`,
      method: "POST",
      path: pubType.list_path!,
      schema: registry.schema(pubType.create_schema),
      plugin,
      initial: version !== null ? {repository_version: href} : {repository: href},
      submitLabel: "Publish",
    });
  };
  const distribute = () => {
    if (!distType) return;
    const source = type?.category === "publications" ? "publication" : version !== null ? "repository_version" : "repository";
    const baseName = (displayName(item, href) || "dist").replace(/[@\s]/g, "-");
    setForm({
      title: `Distribute ${name}`,
      label: `Distribute ${name}`,
      method: "POST",
      path: distType.list_path!,
      schema: registry.schema(distType.create_schema),
      plugin,
      initial: {name: baseName, base_path: baseName, [source]: href},
      submitLabel: "Create distribution",
    });
  };
  const edit = () =>
    type &&
    setForm({
      title: `Edit ${name}`,
      label: `Update ${name}`,
      method: "PATCH",
      path: href,
      schema: registry.schema(type.update_schema),
      plugin,
      editHref: href,
    });
  const remove = () =>
    setConfirm({
      title: `Delete ${name}?`,
      danger: true,
      body: <p className="text-sm">This cannot be undone.</p>,
      run: async () => {
        const resp = await pulp.delete(href);
        track(resp, `Delete ${name}`);
        navigate(repoOfVersion ? itemUrl(ctx, repoOfVersion) : type?.list_path ? listUrl(ctx, type.id) : base(ctx));
      },
    });
  const rollback = () =>
    repoOfVersion &&
    setConfirm({
      title: `Roll back to version ${version}?`,
      body: <p className="text-sm">A new latest version is created with the content of version {version}.</p>,
      run: async () => {
        const resp = await pulp.post(`${repoOfVersion}modify/`, {base_version: href});
        track(resp, `Roll back to version ${version}`);
      },
    });

  const otherActions = (type?.actions ?? []).filter(
    (a) => !DEDICATED.has(a.verb) && !type?.children.some((c) => registry.get(c)?.name === a.verb) && (a.method === "POST" ? !info.readonly : !!a.response_schema),
  );
  const postActions = otherActions.filter((a) => a.method === "POST" && (allowed(a.verb) || allowed("change")));
  const getActions = otherActions.filter((a) => a.method === "GET");
  const canEdit = !!type?.update_schema && type.ops.partial_update && allowed("change");
  const canDelete = !!type?.ops.delete && allowed("delete");

  const crumbs: Crumb[] = [{label: "Home", to: base(ctx)}];
  if (repoOfVersion && parentType) {
    crumbs.push({label: typeTitle(parentType), to: listUrl(ctx, parentType.id)});
    crumbs.push({label: displayName(owningRepo.data, repoOfVersion), to: itemUrl(ctx, repoOfVersion)});
  } else if (type?.list_path) crumbs.push({label: typeTitle(type), to: listUrl(ctx, type.id)});
  crumbs.push({label: name});

  const usage = type ? usageLines(type, item) : [];
  const children = (type?.children ?? []).map((c) => registry.get(c)).filter((c): c is ResourceType => !!c);
  const versionHref = version !== null ? href : isRepository ? (item.latest_version_href as string | undefined) : undefined;
  const genericContent = registry.get("content");
  const hasRoles = !!type?.actions.some((a) => a.verb === "list_roles");
  const pubsType = registry.get("publications");
  const distsType = registry.get("distributions");

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        actions={
          <>
            {syncAction && allowed("sync") && (
              <Button onClick={() => openAction(syncAction)}>
                <RefreshCw />
                Sync
              </Button>
            )}
            {uploadTypes.length > 0 && (
              <Button variant="outline" onClick={() => setUpload(true)}>
                <CloudUpload />
                Upload
              </Button>
            )}
            {pubType && can("add", pubType) && (
              <Button variant="outline" onClick={publish}>
                <Rocket />
                Publish
              </Button>
            )}
            {distType && can("add", distType) && (
              <Button variant="outline" onClick={distribute}>
                <Globe />
                Distribute
              </Button>
            )}
            {version !== null && canModify && owningRepo.data && !isLatest && (
              <Button variant="outline" onClick={rollback}>
                <Undo2 />
                Roll back
              </Button>
            )}
            {canEdit && (
              <Button variant="outline" onClick={edit}>
                <Pencil />
                Edit
              </Button>
            )}
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button aria-label="More actions" size="icon" variant="outline">
                  <Ellipsis />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="min-w-48">
                {!simple && postActions.length > 0 && (
                  <>
                    <DropdownMenuGroup>
                      <DropdownMenuLabel>Actions</DropdownMenuLabel>
                      {postActions.map((a) => (
                        <DropdownMenuItem key={a.verb} onSelect={() => openAction(a)}>
                          {titleize(a.verb)}
                        </DropdownMenuItem>
                      ))}
                    </DropdownMenuGroup>
                    <DropdownMenuSeparator />
                  </>
                )}
                {!simple && getActions.length > 0 && (
                  <>
                    <DropdownMenuGroup>
                      <DropdownMenuLabel>Reports</DropdownMenuLabel>
                      {getActions.map((a) => (
                        <DropdownMenuItem key={a.verb} onSelect={() => openAction(a)}>
                          {titleize(a.verb)}
                        </DropdownMenuItem>
                      ))}
                    </DropdownMenuGroup>
                    <DropdownMenuSeparator />
                  </>
                )}
                <DropdownMenuGroup>
                  <DropdownMenuLabel>Resource</DropdownMenuLabel>
                  <DropdownMenuItem onSelect={() => void query.refetch()}>
                    <RefreshCw />
                    Refresh
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onSelect={() => void navigator.clipboard.writeText(href).then(() => toast.success("Href copied", {duration: 1500}))}
                  >
                    <Copy />
                    Copy href
                  </DropdownMenuItem>
                </DropdownMenuGroup>
                {canDelete && (
                  <>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem variant="destructive" onSelect={remove}>
                      <Trash2 />
                      Delete
                    </DropdownMenuItem>
                  </>
                )}
              </DropdownMenuContent>
            </DropdownMenu>
          </>
        }
        chips={
          type && (
            <>
              {plugin !== "core" && <Badge variant="secondary">{pluginLabel(plugin)}</Badge>}
              <Badge variant="outline">{singular(type.parent ? titleize(type.name) : typeLabel(type))}</Badge>
            </>
          )
        }
        crumbs={crumbs}
        subtitle={
          simple ? (
            typeof item.description === "string" && item.description ? item.description : undefined
          ) : (
            <span className="inline-flex items-center gap-1 font-mono text-xs">
              {href}
              <CopyButton label="Copy href" text={href} />
            </span>
          )
        }
        title={name}
      />

      <Tabs value={simple && (tab === "roles" || tab === "json") ? "overview" : tab} onValueChange={setTab}>
        <div className="overflow-x-auto border-b">
          <TabsList aria-label="Sections" variant="line">
            <TabsTrigger value="overview">Overview</TabsTrigger>
            {versionHref && genericContent && <TabsTrigger value="content">Content</TabsTrigger>}
            {children.map((c) => (
              <TabsTrigger key={c.id} value={`child:${c.id}`}>
                {titleize(c.name)}
              </TabsTrigger>
            ))}
            {isRepository && <TabsTrigger value="relations">Publishing</TabsTrigger>}
            {!simple && hasRoles && <TabsTrigger value="roles">Roles</TabsTrigger>}
            {!simple && <TabsTrigger value="json">JSON</TabsTrigger>}
          </TabsList>
        </div>

        <TabsContent className="flex flex-col gap-4 pt-2" value="overview">
          {usage.length > 0 && (
            <Section title="How to use it">
              <ul className="flex flex-col gap-2">
                {usage.map((line) => (
                  <li key={line} className="flex items-start gap-2 rounded-lg bg-muted/60 px-3 py-2">
                    <pre className="flex-1 overflow-x-auto whitespace-pre-wrap font-mono text-xs leading-5">{line}</pre>
                    <CopyButton text={line} />
                  </li>
                ))}
              </ul>
            </Section>
          )}
          {versionHref && (
            <Section title={version !== null ? "Content changes" : "Latest version"}>
              <ContentSummary versionHref={versionHref} />
            </Section>
          )}
          <Section title="Details">
            <Details item={item} simple={simple} />
          </Section>
        </TabsContent>

        {versionHref && genericContent && (
          <TabsContent className="pt-2" value="content">
            <ResourceList
              embedded
              baseQuery={{repository_version: versionHref}}
              path={genericContent.list_path!}
              type={genericContent}
              versionContext={repoForModify ? {repository: repoForModify, version: versionHref} : undefined}
            />
          </TabsContent>
        )}
        {children.map((c) => (
          <TabsContent key={c.id} className="pt-2" value={`child:${c.id}`}>
            <ResourceList embedded={false} owner={href} path={`${href}${c.parent?.[1] ?? `${c.name}/`}`} type={c} />
          </TabsContent>
        ))}
        {isRepository && (
          <TabsContent className="flex flex-col gap-6 pt-2" value="relations">
            {pubsType && (
              <div className="flex flex-col gap-2">
                <h2 className="text-lg font-semibold">Publications</h2>
                <ResourceList embedded baseQuery={{repository: href}} path={pubsType.list_path!} type={pubsType} />
              </div>
            )}
            {distsType && (
              <div className="flex flex-col gap-2">
                <h2 className="text-lg font-semibold">Distributions</h2>
                <ResourceList embedded baseQuery={{repository: href}} path={distsType.list_path!} type={distsType} />
              </div>
            )}
          </TabsContent>
        )}
        {hasRoles && (
          <TabsContent className="pt-2" value="roles">
            <RolesPanel href={href} name={name} />
          </TabsContent>
        )}
        <TabsContent className="pt-2" value="json">
          <JsonView value={item} />
        </TabsContent>
      </Tabs>

      <FormModal spec={form} onClose={() => setForm(null)} />
      <ConfirmDialog
        confirmLabel={confirm?.danger ? "Delete" : "Confirm"}
        danger={confirm?.danger}
        isOpen={!!confirm}
        title={confirm?.title ?? ""}
        onConfirm={() => confirm?.run()}
        onOpenChange={(open) => !open && setConfirm(null)}
      >
        {confirm?.body}
      </ConfirmDialog>
      <ResultDialog title={result?.title ?? ""} value={result?.value ?? null} onClose={() => setResult(null)} />
      {upload && <UploadDialog repository={href} onClose={() => setUpload(false)} />}
    </div>
  );
}
