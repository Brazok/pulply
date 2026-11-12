// Paginated list of one resource type (or a nested collection): server-side search,
// filters and ordering, selection with bulk actions, creation and upload.

import {
  ArrowsRotateRight,
  CloudArrowUpIn,
  Funnel,
  Plus,
  TrashBin,
  Xmark,
} from "@gravity-ui/icons";
import {
  Button,
  Checkbox,
  Chip,
  EmptyState,
  Input,
  Label,
  ListBox,
  Pagination,
  Popover,
  SearchField,
  Select,
  type Selection,
  type SortDescriptor,
  Spinner,
  Table,
  TextField,
  toast,
} from "@heroui/react";
import {keepPreviousData, useQuery} from "@tanstack/react-query";
import {type ReactNode, useEffect, useMemo, useState} from "react";
import {useNavigate} from "react-router";

import type {Json} from "../lib/api";
import {columnsFor} from "../lib/columns";
import {displayName, singular, typeLabel, typeTitle} from "../lib/format";
import type {Param, ResourceType} from "../lib/registry";
import {itemUrl} from "../lib/routes";
import {errorText, useSession} from "../lib/session";
import {AddToRepository} from "./AddToRepository";
import {ConfirmDialog} from "./dialogs";
import {type FormSpec, FormModal} from "./SchemaForm";
import {UploadDialog} from "./UploadDialog";

const PAGE_SIZES = [25, 50, 100];
const TASK_STATES = ["running", "waiting", "failed", "completed", "canceled"];

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
  const columns = useMemo(() => columnsFor(type, registry.readSchema(type), registry), [type, registry]);
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
  const [selected, setSelected] = useState<Selection>(new Set());
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
  const selectedHrefs: string[] =
    selected === "all" ? items.map((i) => i.pulp_href as string) : [...selected].map(String);

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
              <Button variant="secondary" onPress={() => setUploading(true)}>
                <CloudArrowUpIn />
                Upload
              </Button>
            )}
            {canCreate && (
              <Button onPress={openCreate}>
                <Plus />
                {nestedCreate ? "Add" : "Create"}
              </Button>
            )}
          </div>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-2">
        {search && (
          <SearchField aria-label="Search" className="w-full sm:w-72" value={text} onChange={setText}>
            <SearchField.Group>
              <SearchField.SearchIcon />
              <SearchField.Input placeholder={search === "q" ? "Filter expression…" : "Search…"} />
              <SearchField.ClearButton />
            </SearchField.Group>
          </SearchField>
        )}
        {isTasks && (
          <div className="flex flex-wrap gap-1">
            {["", ...TASK_STATES].map((s) => (
              <Button
                key={s || "all"}
                size="sm"
                variant={(filters.state ?? "") === s ? "primary" : "ghost"}
                onPress={() => {
                  setFilters(({state: _, ...rest}) => (s ? {...rest, state: s} : rest));
                  setPage(1);
                }}
              >
                {s || "All"}
              </Button>
            ))}
          </div>
        )}
        <FilterPopover
          params={filterParams(type.list_params).filter((p) => p.name !== search)}
          onAdd={(k, v) => {
            setFilters((f) => ({...f, [k]: v}));
            setPage(1);
          }}
        />
        {activeFilters
          .filter(([k]) => !(isTasks && k === "state"))
          .map(([k, v]) => (
            <Chip key={k} size="md" variant="secondary">
              <Chip.Label>
                {k} = {v}
              </Chip.Label>
              <button
                aria-label={`Remove filter ${k}`}
                className="ms-1 opacity-60 hover:opacity-100"
                onClick={() => {
                  setFilters(({[k]: _, ...rest}) => rest);
                  setPage(1);
                }}
              >
                <Xmark className="size-3" />
              </button>
            </Chip>
          ))}
        <div className="ms-auto flex items-center gap-2">
          {list.isFetching && <Spinner size="sm" />}
          <Button isIconOnly aria-label="Refresh" size="sm" variant="ghost" onPress={() => list.refetch()}>
            <ArrowsRotateRight />
          </Button>
        </div>
      </div>

      {selectedHrefs.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 rounded-2xl bg-accent-soft px-4 py-2 text-sm">
          <span className="font-medium">{selectedHrefs.length} selected</span>
          <div className="ms-auto flex flex-wrap gap-2">
            {type.category === "content" && canModify && !versionContext && (
              <Button size="sm" variant="secondary" onPress={() => setAdding(true)}>
                <Plus />
                Add to repository
              </Button>
            )}
            {versionContext && canModify && (
              <Button size="sm" variant="danger-soft" onPress={() => setConfirmRemove(true)}>
                Remove from repository
              </Button>
            )}
            {isTasks && (
              <Button size="sm" variant="danger-soft" onPress={() => setConfirmCancel(true)}>
                Cancel tasks
              </Button>
            )}
            {deletable && !isTasks && can("delete", type) && (
              <Button size="sm" variant="danger" onPress={() => setConfirmDelete(true)}>
                <TrashBin />
                {nestedCreate ? "Remove" : "Delete"}
              </Button>
            )}
            <Button size="sm" variant="ghost" onPress={() => setSelected(new Set())}>
              Clear
            </Button>
          </div>
        </div>
      )}

      {list.error ? (
        <div className="rounded-2xl bg-danger/10 p-4 text-danger text-sm">{errorText(list.error)}</div>
      ) : (
        <Table>
          <Table.ScrollContainer>
            <Table.Content
              aria-label={typeTitle(type)}
              className="min-w-[640px]"
              selectedKeys={selected}
              selectionMode="multiple"
              sortDescriptor={sort}
              onRowAction={(key) => navigate(itemUrl(ctx, String(key)))}
              onSelectionChange={setSelected}
              onSortChange={(d) => {
                setSort(d);
                setPage(1);
              }}
            >
              <Table.Header>
                <Table.Column className="pe-0 w-10">
                  <Checkbox aria-label="Select all" slot="selection">
                    <Checkbox.Content>
                      <Checkbox.Control>
                        <Checkbox.Indicator />
                      </Checkbox.Control>
                    </Checkbox.Content>
                  </Checkbox>
                </Table.Column>
                {columns.map((c) => {
                  const sortable = !!c.sort && (ordering.includes(c.sort) || ordering.includes(`-${c.sort}`));
                  return (
                    <Table.Column key={c.id} allowsSorting={sortable} id={c.id} isRowHeader={c.primary}>
                      {({sortDirection}) =>
                        sortable ? (
                          <Table.SortableColumnHeader sortDirection={sortDirection}>{c.header}</Table.SortableColumnHeader>
                        ) : (
                          c.header
                        )
                      }
                    </Table.Column>
                  );
                })}
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <EmptyState className="flex flex-col items-center justify-center gap-2 py-10 text-center">
                    {list.isLoading ? (
                      <Spinner />
                    ) : (
                      <span className="text-sm text-muted">
                        {Object.keys(filters).length ? "Nothing matches these filters." : "Nothing here yet."}
                      </span>
                    )}
                  </EmptyState>
                )}
              >
                {items.map((item) => (
                  <Table.Row key={item.pulp_href as string} className="cursor-pointer" id={item.pulp_href as string}>
                    <Table.Cell className="pe-0">
                      <Checkbox aria-label={`Select ${displayName(item)}`} slot="selection" variant="secondary">
                        <Checkbox.Content>
                          <Checkbox.Control>
                            <Checkbox.Indicator />
                          </Checkbox.Control>
                        </Checkbox.Content>
                      </Checkbox>
                    </Table.Cell>
                    {columns.map((c) => (
                      <Table.Cell key={c.id}>{c.render(item)}</Table.Cell>
                    ))}
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
          <Table.Footer>
            <div className="flex w-full flex-wrap items-center justify-between gap-3 px-2 py-1">
              <span className="text-sm text-muted">
                {total === 0
                  ? "No results"
                  : `${(page - 1) * pageSize + 1}–${Math.min(page * pageSize, total)} of ${total.toLocaleString()}`}
              </span>
              <div className="flex items-center gap-3">
                <Select
                  aria-label="Rows per page"
                  className="w-24"
                  value={pageSize}
                  onChange={(k) => {
                    setPageSize(Number(k));
                    setPage(1);
                  }}
                >
                  <Select.Trigger>
                    <Select.Value />
                    <Select.Indicator />
                  </Select.Trigger>
                  <Select.Popover>
                    <ListBox>
                      {PAGE_SIZES.map((n) => (
                        <ListBox.Item key={n} id={n} textValue={`${n}`}>
                          {n}
                          <ListBox.ItemIndicator />
                        </ListBox.Item>
                      ))}
                    </ListBox>
                  </Select.Popover>
                </Select>
                <Pages page={page} pages={pages} onChange={setPage} />
              </div>
            </div>
          </Table.Footer>
        </Table>
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
          if (failed.length) toast.danger(`${failed.length} deletion(s) failed`, {description: errorText(failed[0].reason)});
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
    <Pagination size="sm">
      <Pagination.Content>
        <Pagination.Item>
          <Pagination.Previous isDisabled={page <= 1} onPress={() => onChange(page - 1)}>
            <Pagination.PreviousIcon />
          </Pagination.Previous>
        </Pagination.Item>
        {shown.map((p, i) => (
          <span key={p} className="contents">
            {i > 0 && shown[i - 1] !== p - 1 && (
              <Pagination.Item>
                <Pagination.Ellipsis />
              </Pagination.Item>
            )}
            <Pagination.Item>
              <Pagination.Link isActive={p === page} onPress={() => onChange(p)}>
                {p}
              </Pagination.Link>
            </Pagination.Item>
          </span>
        ))}
        <Pagination.Item>
          <Pagination.Next isDisabled={page >= pages} onPress={() => onChange(page + 1)}>
            <Pagination.NextIcon />
          </Pagination.Next>
        </Pagination.Item>
      </Pagination.Content>
    </Pagination>
  );
}

function FilterPopover({params, onAdd}: {params: Param[]; onAdd: (key: string, value: string) => void}) {
  const [name, setName] = useState<string | null>(null);
  const [value, setValue] = useState("");
  const [open, setOpen] = useState(false);
  const param = params.find((p) => p.name === name);
  if (!params.length) return null;
  const choices = param?.ty === "boolean" ? ["true", "false"] : (param?.enum_values ?? []);
  return (
    <Popover isOpen={open} onOpenChange={setOpen}>
      <Button size="sm" variant="secondary">
        <Funnel />
        Filter
      </Button>
      <Popover.Content className="w-80">
        <Popover.Dialog className="flex flex-col gap-3 p-1">
          <Select
            fullWidth
            placeholder="Choose a field"
            value={name}
            onChange={(k) => {
              setName(k === null ? null : String(k));
              setValue("");
            }}
          >
            <Label>Field</Label>
            <Select.Trigger>
              <Select.Value />
              <Select.Indicator />
            </Select.Trigger>
            <Select.Popover>
              <ListBox>
                {params.map((p) => (
                  <ListBox.Item key={p.name} id={p.name} textValue={p.name}>
                    {p.name}
                    <ListBox.ItemIndicator />
                  </ListBox.Item>
                ))}
              </ListBox>
            </Select.Popover>
          </Select>
          {param && choices.length > 0 && (
            <Select fullWidth placeholder="Value" value={value || null} onChange={(k) => setValue(k === null ? "" : String(k))}>
              <Label>Value</Label>
              <Select.Trigger>
                <Select.Value />
                <Select.Indicator />
              </Select.Trigger>
              <Select.Popover>
                <ListBox>
                  {choices.map((c) => (
                    <ListBox.Item key={c} id={c} textValue={c}>
                      {c}
                      <ListBox.ItemIndicator />
                    </ListBox.Item>
                  ))}
                </ListBox>
              </Select.Popover>
            </Select>
          )}
          {param && !choices.length && (
            <TextField fullWidth value={value} onChange={setValue}>
              <Label>Value</Label>
              <Input placeholder={param.ty === "array" ? "a,b,c" : "Value"} />
            </TextField>
          )}
          {param?.description && <p className="text-xs text-muted">{param.description.split("\n")[0]}</p>}
          <Button
            isDisabled={!name || !value}
            onPress={() => {
              onAdd(name!, value);
              setOpen(false);
              setName(null);
              setValue("");
            }}
          >
            Apply
          </Button>
        </Popover.Dialog>
      </Popover.Content>
    </Popover>
  );
}
