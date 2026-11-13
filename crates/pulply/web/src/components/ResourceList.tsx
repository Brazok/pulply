// Paginated list of one resource type (or a nested collection): server-side search,
// filters and ordering, selection with bulk actions, creation and upload.

import {keepPreviousData, useQuery} from "@tanstack/react-query";
import {
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  ChevronLeft,
  ChevronRight,
  CloudUpload,
  ListFilter,
  MoreHorizontal,
  Plus,
  RefreshCw,
  Search,
  Trash2,
  X,
} from "lucide-react";
import {Fragment, type ReactNode, useEffect, useMemo, useState} from "react";
import {useNavigate} from "react-router";
import {toast} from "sonner";

import type {Json} from "../lib/api";
import {columnsFor} from "../lib/columns";
import {displayName, singular, typeLabel, typeTitle} from "../lib/format";
import type {Param, ResourceType} from "../lib/registry";
import {useMode} from "../lib/mode";
import {itemUrl} from "../lib/routes";
import {errorText, useSession} from "../lib/session";
import {AddToRepository} from "./AddToRepository";
import {ConfirmDialog} from "./dialogs";
import {type FormSpec, FormModal} from "./SchemaForm";
import {Badge} from "./ui/badge";
import {Button} from "./ui/button";
import {Checkbox} from "./ui/checkbox";
import {Field, FieldLabel} from "./ui/field";
import {Input} from "./ui/input";
import {InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput} from "./ui/input-group";
import {Popover, PopoverContent, PopoverTrigger} from "./ui/popover";
import {Select, SelectContent, SelectItem, SelectTrigger, SelectValue} from "./ui/select";
import {Spinner} from "./ui/spinner";
import {TableSkeletonRows} from "./loading";
import {cn} from "../lib/utils";
import {Table, TableBody, TableCell, TableHead, TableHeader, TableRow} from "./ui/table";
import {UploadDialog} from "./UploadDialog";

const PAGE_SIZES = [25, 50, 100];
const TASK_STATES = ["running", "waiting", "failed", "completed", "canceled"];

interface SortDescriptor {
  column: string;
  direction: "ascending" | "descending";
}

export interface ListProps {
  type: ResourceType;
  /** List endpoint (nested collections: `<owner href>versions/`). */
  path: string;
  /** Fixed filters (`repository_version=…` for the content of a version). */
  baseQuery?: Record<string, string>;
  /** Initial user filters (from the URL). */
  initialFilters?: Record<string, string>;
  onFiltersChange?: (filters: Record<string, string>) => void;
  /** Owner of a nested collection (creation through `POST <owner>/<name>/`). */
  owner?: string;
  /** Repository + version whose content is listed: enables "Remove from repository". */
  versionContext?: {repository: string; version: string};
  embedded?: boolean;
  title?: ReactNode;
  actions?: ReactNode;
}

/** Search parameter of a type: `name__icontains`, else another `__icontains`/`__contains`, else `q`. */
function searchParam(params: Param[]): string | null {
  const names = params.map((p) => p.name);
  return (
    ["name__icontains", "name__contains", "relative_path__contains", "username__icontains", "digest", "q"].find((n) =>
      names.includes(n),
    ) ??
    names.find((n) => n.endsWith("__icontains")) ??
    null
  );
}

/** Params worth offering as filters (exact or useful lookups, no pagination/ordering noise). */
function filterParams(params: Param[]): Param[] {
  return params.filter(
    (p) =>
      !["ordering", "fields", "exclude_fields", "offset", "limit", "q"].includes(p.name) &&
      !/__(gt|gte|lt|lte|range|ne|regex|iregex|startswith|istartswith)$/.test(p.name),
  );
}

export function ResourceList(props: ListProps) {
  const {type, path, baseQuery, owner, versionContext} = props;
  const {registry, pulp, ctx, can, refresh, track} = useSession();
  const navigate = useNavigate();
  const {simple} = useMode();
  const allColumns = useMemo(() => columnsFor(type, registry.readSchema(type), registry), [type, registry]);
  // Simple mode: the first four columns, plus the creation date.
  const columns = useMemo(() => {
    if (!simple || allColumns.length <= 5) return allColumns;
    const created = allColumns.find((c) => c.id === "pulp_created");
    const rest = allColumns.filter((c) => c !== created).slice(0, 4);
    return created ? [...rest, created] : rest;
  }, [allColumns, simple]);
  /** Bulk selection and actions: advanced mode. */
  const selectable = !simple;
  const ordering = type.list_params.find((p) => p.name === "ordering")?.enum_values ?? [];
  const search = searchParam(type.list_params);
  const isTasks = type.category === "tasks";

  const [filters, setFilters] = useState<Record<string, string>>(props.initialFilters ?? {});
  const [text, setText] = useState(filters[search ?? ""] ?? "");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(25);
  const [sort, setSort] = useState<SortDescriptor | undefined>(() =>
    ordering.includes("-pulp_created") ? {column: "pulp_created", direction: "descending"} : undefined,
  );
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [form, setForm] = useState<FormSpec | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [confirmCancel, setConfirmCancel] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [adding, setAdding] = useState(false);

  const {onFiltersChange} = props;
  useEffect(() => onFiltersChange?.(filters), [filters, onFiltersChange]);
  // Debounced search box.
  useEffect(() => {
    if (!search) return;
    const timer = window.setTimeout(() => {
      setFilters((f) => {
        const next = {...f};
        if (text) next[search] = text;
        else delete next[search];
        return JSON.stringify(next) === JSON.stringify(f) ? f : next;
      });
      setPage(1);
    }, 300);
    return () => window.clearTimeout(timer);
  }, [text, search]);

  const sortColumn = columns.find((c) => c.id === sort?.column);
  const orderingValue = sortColumn?.sort
    ? `${sort?.direction === "descending" ? "-" : ""}${sortColumn.sort}`
    : undefined;
  const query = {
    ...baseQuery,
    ...filters,
    limit: pageSize,
    offset: (page - 1) * pageSize,
    ordering: orderingValue && ordering.includes(orderingValue) ? orderingValue : undefined,
  };
  const list = useQuery({
    queryKey: ["pulp", ctx, "list", path, query],
    queryFn: () => pulp.list(path, query),
    placeholderData: keepPreviousData,
    refetchInterval: isTasks ? refresh : false,
  });
  const items = list.data?.results ?? [];
  const total = list.data?.count ?? 0;
  const pages = Math.max(1, Math.ceil(total / pageSize));
  const selectedHrefs: string[] = selectable ? [...selected] : [];
  const pageHrefs = items.map((i) => i.pulp_href as string);
  const pageSelected = pageHrefs.filter((h) => selected.has(h)).length;
  const toggle = (hrefs: string[], on: boolean) =>
    setSelected((cur) => {
      const next = new Set(cur);
      hrefs.forEach((h) => (on ? next.add(h) : next.delete(h)));
      return next;
    });
  const toggleSort = (column: string) => {
    setSort((cur) =>
      cur?.column === column && cur.direction === "ascending" ? {column, direction: "descending"} : {column, direction: "ascending"},
    );
    setPage(1);
  };

  // Creation: own create endpoint, or the owner's nested create action (`groups/…/users/`).
  const nestedCreate = owner
    ? registry.typeForHref(owner)?.actions.find((a) => a.method === "POST" && a.verb === type.name)
    : undefined;
  const canCreate = nestedCreate ? can("change", registry.typeForHref(owner!)) : type.ops.create && !owner && can("add", type);
  const uploadTypes = type.category === "content" ? registry.uploadTypes(type.plugin).filter((t) => t.id === type.id) : [];
  const repoType = type.category === "content" ? registry.pluginType("repositories", type.plugin, false) : undefined;
  const canModify = !!repoType?.actions.some((a) => a.verb === "modify") && can("modify", repoType);
  const deletable = type.ops.delete || !!nestedCreate;

  /** Group users are listed as users (`/users/2/`); removal targets the membership. */
  const deleteTarget = (h: string) =>
    nestedCreate && !h.startsWith(path) ? `${path}${h.replace(/\/+$/, "").split("/").at(-1)}/` : h;

  const openCreate = () => {
    const label = singular(typeLabel(type));
    if (nestedCreate) {
      setForm({
        title: `Add to ${typeLabel(type).toLowerCase()}`,
        label: `Add ${label.toLowerCase()}`,
        method: "POST",
        path,
        schema: registry.schema(nestedCreate.body_schema),
        plugin: type.plugin,
        multipart: nestedCreate.multipart,
      });
      return;
    }
    setForm({
      title: `New ${singular(typeTitle(type)).toLowerCase()}`,
      label: `Create ${label.toLowerCase()}`,
      method: "POST",
      path: type.list_path!,
      schema: registry.schema(type.create_schema),
      plugin: type.plugin,
      multipart: type.create_multipart,
      submitLabel: "Create",
      showResult: false,
      onDone: (resp) => {
        const href = (resp as Json)?.pulp_href;
        if (typeof href === "string") navigate(itemUrl(ctx, href));
      },
    });
  };

  const activeFilters = Object.entries(filters).filter(([k]) => k !== search);

  return (
    <div className="flex flex-col gap-4">
      {!props.embedded && (
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div>{props.title}</div>
          <div className="flex flex-wrap gap-2">
            {props.actions}
            {uploadTypes.length > 0 && can("add", type) && (
              <Button variant="outline" onClick={() => setUploading(true)}>
                <CloudUpload />
                Upload
              </Button>
            )}
            {canCreate && (
              <Button onClick={openCreate}>
                <Plus />
                {nestedCreate ? "Add" : "Create"}
              </Button>
            )}
          </div>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-2">
        {search && (
          <InputGroup className="w-full sm:max-w-xl sm:flex-1">
            <InputGroupAddon>
              <Search />
            </InputGroupAddon>
            <InputGroupInput
              aria-label="Search"
              placeholder={search === "q" ? "Filter expression…" : "Search…"}
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && text && (e.preventDefault(), setText(""))}
            />
            {text && (
              <InputGroupAddon align="inline-end">
                <InputGroupButton aria-label="Clear search" size="icon-xs" onClick={() => setText("")}>
                  <X />
                </InputGroupButton>
              </InputGroupAddon>
            )}
          </InputGroup>
        )}
        {isTasks && (
          <div className="flex flex-wrap gap-1">
            {["", ...TASK_STATES].map((s) => (
              <Button
                key={s || "all"}
                size="sm"
                variant={(filters.state ?? "") === s ? "default" : "ghost"}
                onClick={() => {
                  setFilters(({state: _, ...rest}) => (s ? {...rest, state: s} : rest));
                  setPage(1);
                }}
              >
                {s || "All"}
              </Button>
            ))}
          </div>
        )}
        {!simple && (
          <FilterPopover
            params={filterParams(type.list_params).filter((p) => p.name !== search)}
            onAdd={(k, v) => {
              setFilters((f) => ({...f, [k]: v}));
              setPage(1);
            }}
          />
        )}
        {activeFilters
          .filter(([k]) => !(isTasks && k === "state"))
          .map(([k, v]) => (
            <Badge key={k} className="h-6 gap-1 pe-1" variant="secondary">
              {k} = {v}
              <button
                aria-label={`Remove filter ${k}`}
                className="rounded-full opacity-60 hover:opacity-100"
                onClick={() => {
                  setFilters(({[k]: _, ...rest}) => rest);
                  setPage(1);
                }}
              >
                <X className="size-3" />
              </button>
            </Badge>
          ))}
        <div className="ms-auto flex items-center gap-2">
          {list.isFetching && <Spinner className="text-muted-foreground" />}
          <Button aria-label="Refresh" size="icon-sm" variant="ghost" onClick={() => list.refetch()}>
            <RefreshCw />
          </Button>
        </div>
      </div>

      {selectedHrefs.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 rounded-lg border border-primary/20 bg-primary/5 px-4 py-2 text-sm">
          <span className="font-medium">{selectedHrefs.length} selected</span>
          <div className="ms-auto flex flex-wrap gap-2">
            {type.category === "content" && canModify && !versionContext && (
              <Button size="sm" variant="outline" onClick={() => setAdding(true)}>
                <Plus />
                Add to repository
              </Button>
            )}
            {versionContext && canModify && (
              <Button size="sm" variant="destructive" onClick={() => setConfirmRemove(true)}>
                Remove from repository
              </Button>
            )}
            {isTasks && (
              <Button size="sm" variant="destructive" onClick={() => setConfirmCancel(true)}>
                Cancel tasks
              </Button>
            )}
            {deletable && !isTasks && can("delete", type) && (
              <Button size="sm" variant="destructive" onClick={() => setConfirmDelete(true)}>
                <Trash2 />
                {nestedCreate ? "Remove" : "Delete"}
              </Button>
            )}
            <Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>
              Clear
            </Button>
          </div>
        </div>
      )}

      {list.error ? (
        <div className="rounded-lg bg-destructive/10 p-4 text-sm text-destructive">{errorText(list.error)}</div>
      ) : (
        <div className="flex flex-col rounded-lg border">
          <Table aria-label={typeTitle(type)} className="min-w-[640px]">
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                {selectable && (
                  <TableHead className="w-10 pe-0">
                    <Checkbox
                      aria-label="Select all"
                      checked={pageSelected === 0 ? false : pageSelected === pageHrefs.length ? true : "indeterminate"}
                      disabled={!pageHrefs.length}
                      onCheckedChange={(on) => toggle(pageHrefs, on === true)}
                    />
                  </TableHead>
                )}
                {columns.map((c) => {
                  const sortable = !!c.sort && (ordering.includes(c.sort) || ordering.includes(`-${c.sort}`));
                  const direction = sort?.column === c.id ? sort.direction : undefined;
                  return (
                    <TableHead
                      key={c.id}
                      aria-sort={direction ?? (sortable ? "none" : undefined)}
                    >
                      {sortable ? (
                        <Button className="group/sort -ms-2 h-7 px-2" size="sm" variant="ghost" onClick={() => toggleSort(c.id)}>
                          {c.header}
                          {direction === "ascending" ? (
                            <ArrowUp />
                          ) : direction === "descending" ? (
                            <ArrowDown />
                          ) : (
                            <ArrowUpDown className="opacity-0 transition-opacity group-hover/sort:opacity-50 group-focus-visible/sort:opacity-50" />
                          )}
                        </Button>
                      ) : (
                        c.header
                      )}
                    </TableHead>
                  );
                })}
              </TableRow>
            </TableHeader>
            <TableBody className={cn("transition-opacity", list.isPlaceholderData && "opacity-60")}>
              {!items.length && list.isLoading && <TableSkeletonRows columns={columns.length + (selectable ? 1 : 0)} />}
              {!items.length && !list.isLoading && (
                <TableRow className="hover:bg-transparent">
                  <TableCell className="py-10 text-center" colSpan={columns.length + (selectable ? 1 : 0)}>
                    <span className="text-sm text-muted-foreground">
                      {Object.keys(filters).length ? "Nothing matches these filters." : "Nothing here yet."}
                    </span>
                  </TableCell>
                </TableRow>
              )}
              {items.map((item) => {
                const href = item.pulp_href as string;
                return (
                  <TableRow
                    key={href}
                    className="cursor-pointer"
                    data-state={selected.has(href) ? "selected" : undefined}
                    onClick={(e) => {
                      // Links, buttons and the selection box inside the row keep their own behaviour.
                      if ((e.target as HTMLElement).closest("a,button,input,[role=checkbox]")) return;
                      navigate(itemUrl(ctx, href));
                    }}
                  >
                    {selectable && (
                      <TableCell className="pe-0">
                        <Checkbox
                          aria-label={`Select ${displayName(item)}`}
                          checked={selected.has(href)}
                          onCheckedChange={(on) => toggle([href], on === true)}
                        />
                      </TableCell>
                    )}
                    {columns.map((c) => (
                      <TableCell key={c.id} className={c.primary ? "font-medium" : undefined}>
                        {c.render(item)}
                      </TableCell>
                    ))}
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
          <div className="flex w-full flex-wrap items-center justify-between gap-3 border-t px-3 py-2">
            <span className="text-sm text-muted-foreground">
              {list.isLoading
                ? "Loading…"
                : total === 0
                ? "No results"
                : `${(page - 1) * pageSize + 1}–${Math.min(page * pageSize, total)} of ${total.toLocaleString()}`}
            </span>
            <div className="flex items-center gap-3">
              <Select
                value={String(pageSize)}
                onValueChange={(k) => {
                  setPageSize(Number(k));
                  setPage(1);
                }}
              >
                <SelectTrigger aria-label="Rows per page" className="w-20" size="sm">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {PAGE_SIZES.map((n) => (
                    <SelectItem key={n} value={String(n)}>
                      {n}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Pages page={page} pages={pages} onChange={setPage} />
            </div>
          </div>
        </div>
      )}

      <FormModal spec={form} onClose={() => setForm(null)} />
      <ConfirmDialog
        danger
        confirmLabel={nestedCreate ? "Remove" : "Delete"}
        isOpen={confirmDelete}
        title={`${nestedCreate ? "Remove" : "Delete"} ${selectedHrefs.length} item${selectedHrefs.length > 1 ? "s" : ""}?`}
        onConfirm={async () => {
          const results = await Promise.allSettled(selectedHrefs.map((h) => pulp.delete(deleteTarget(h))));
          results.forEach((r) => r.status === "fulfilled" && track(r.value, "Delete"));
          const failed = results.filter((r) => r.status === "rejected") as PromiseRejectedResult[];
          if (failed.length) toast.error(`${failed.length} deletion(s) failed`, {description: errorText(failed[0].reason)});
          else toast.success("Deleted");
          setSelected(new Set());
          list.refetch();
        }}
        onOpenChange={setConfirmDelete}
      >
        <p className="text-sm">This cannot be undone.</p>
      </ConfirmDialog>
      <ConfirmDialog
        danger
        confirmLabel="Cancel tasks"
        isOpen={confirmCancel}
        title={`Cancel ${selectedHrefs.length} task(s)?`}
        onConfirm={async () => {
          await Promise.allSettled(selectedHrefs.map((h) => pulp.patch(h, {state: "canceled"})));
          setSelected(new Set());
          list.refetch();
        }}
        onOpenChange={setConfirmCancel}
      />
      {versionContext && (
        <ConfirmDialog
          danger
          confirmLabel="Remove"
          isOpen={confirmRemove}
          title={`Remove ${selectedHrefs.length} unit(s) from the repository?`}
          onConfirm={async () => {
            const resp = await pulp.post(`${versionContext.repository}modify/`, {
              remove_content_units: selectedHrefs,
              base_version: versionContext.version,
            });
            track(resp, "Remove content");
            setSelected(new Set());
          }}
          onOpenChange={setConfirmRemove}
        >
          <p className="text-sm">A new repository version is created without these units.</p>
        </ConfirmDialog>
      )}
      {uploading && <UploadDialog type={type} onClose={() => setUploading(false)} />}
      {adding && repoType && (
        <AddToRepository repoType={repoType} units={selectedHrefs} onClose={() => setAdding(false)} onDone={() => setSelected(new Set())} />
      )}
    </div>
  );
}

function Pages({page, pages, onChange}: {page: number; pages: number; onChange: (p: number) => void}) {
  const shown = useMemo(() => {
    const set = new Set([1, pages, page - 1, page, page + 1].filter((p) => p >= 1 && p <= pages));
    return [...set].sort((a, b) => a - b);
  }, [page, pages]);
  return (
    <nav aria-label="Pagination" className="flex items-center gap-1">
      <Button aria-label="Previous page" disabled={page <= 1} size="icon-sm" variant="ghost" onClick={() => onChange(page - 1)}>
        <ChevronLeft />
      </Button>
      {shown.map((p, i) => (
        <Fragment key={p}>
          {i > 0 && shown[i - 1] !== p - 1 && (
            <span aria-hidden className="flex size-7 items-center justify-center text-muted-foreground">
              <MoreHorizontal className="size-4" />
            </span>
          )}
          <Button
            aria-current={p === page ? "page" : undefined}
            className="min-w-7 px-1.5 tabular-nums"
            size="sm"
            variant={p === page ? "outline" : "ghost"}
            onClick={() => onChange(p)}
          >
            {p}
          </Button>
        </Fragment>
      ))}
      <Button aria-label="Next page" disabled={page >= pages} size="icon-sm" variant="ghost" onClick={() => onChange(page + 1)}>
        <ChevronRight />
      </Button>
    </nav>
  );
}

function FilterPopover({params, onAdd}: {params: Param[]; onAdd: (key: string, value: string) => void}) {
  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [open, setOpen] = useState(false);
  const param = params.find((p) => p.name === name);
  if (!params.length) return null;
  const choices = param?.ty === "boolean" ? ["true", "false"] : (param?.enum_values ?? []);
  const apply = () => {
    onAdd(name, value);
    setOpen(false);
    setName("");
    setValue("");
  };
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button size="sm" variant="outline">
          <ListFilter />
          Filter
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-80">
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            if (name && value) apply();
          }}
        >
          <Field>
            <FieldLabel htmlFor="filter-field">Field</FieldLabel>
            <Select
              value={name}
              onValueChange={(k) => {
                setName(k);
                setValue("");
              }}
            >
              <SelectTrigger className="w-full" id="filter-field">
                <SelectValue placeholder="Choose a field" />
              </SelectTrigger>
              <SelectContent>
                {params.map((p) => (
                  <SelectItem key={p.name} value={p.name}>
                    {p.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          {param && choices.length > 0 && (
            <Field>
              <FieldLabel htmlFor="filter-value">Value</FieldLabel>
              <Select value={value} onValueChange={setValue}>
                <SelectTrigger className="w-full" id="filter-value">
                  <SelectValue placeholder="Value" />
                </SelectTrigger>
                <SelectContent>
                  {choices.map((c) => (
                    <SelectItem key={c} value={c}>
                      {c}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
          )}
          {param && !choices.length && (
            <Field>
              <FieldLabel htmlFor="filter-value">Value</FieldLabel>
              <Input
                id="filter-value"
                placeholder={param.ty === "array" ? "a,b,c" : "Value"}
                value={value}
                onChange={(e) => setValue(e.target.value)}
              />
            </Field>
          )}
          {param?.description && <p className="text-xs text-muted-foreground">{param.description.split("\n")[0]}</p>}
          <Button disabled={!name || !value} type="submit">
            Apply
          </Button>
        </form>
      </PopoverContent>
    </Popover>
  );
}
