// Search-as-you-type picker returning a resource href (pasting an href also works).

import {useQueries, useQuery} from "@tanstack/react-query";
import {Command as CommandPrimitive} from "cmdk";
import {ChevronsUpDown} from "lucide-react";
import {useEffect, useId, useRef, useState} from "react";

import type {Json} from "../lib/api";
import {categoryLabel, displayName, relativeTime, titleize, typeLabel} from "../lib/format";
import {type ResourceType, versionNumber, versionRepository} from "../lib/registry";
import {useSession} from "../lib/session";
import {CommandEmpty, CommandItem, CommandList} from "./ui/command";
import {Field, FieldLabel} from "./ui/field";
import {InputGroup, InputGroupAddon} from "./ui/input-group";
import {Popover, PopoverAnchor, PopoverContent} from "./ui/popover";
import {Spinner} from "./ui/spinner";

/** Repository of a version or publication item. */
function repositoryOf(item: Json): string | undefined {
  const version = typeof item.repository_version === "string" ? item.repository_version : typeof item.number === "number" ? (item.pulp_href as string) : undefined;
  if (version) return versionRepository(version) ?? undefined;
  return typeof item.repository === "string" ? item.repository : undefined;
}

/** Names of the repositories of version and publication items (cached per href). */
function useRepoNames(items: Json[]): Record<string, string> {
  const {pulp, ctx} = useSession();
  const hrefs = [...new Set(items.filter((i) => typeof i.number === "number" || "repository_version" in i).map(repositoryOf).filter((h): h is string => !!h))];
  const names = useQueries({
    queries: hrefs.map((h) => ({
      queryKey: ["pulp", ctx, "pick-name", h],
      queryFn: () => pulp.get(h, {fields: "pulp_href,name"}),
      staleTime: 60_000,
    })),
  });
  return Object.fromEntries(hrefs.map((h, i) => [h, names[i].data ? displayName(names[i].data, h) : ""]));
}

/**
 * Label of an item: versions and publications read "repository · version 3" (they have no
 * name, and "Version 1" alone repeats across repositories); within one repository, "Version 3".
 */
function describe(item: Json, href: string, repoNames: Record<string, string>, scoped: boolean): string {
  const repo = repositoryOf(item);
  const repoName = repo ? repoNames[repo] : "";
  if (typeof item.number === "number") return scoped || !repoName ? `Version ${item.number}` : `${repoName} · version ${item.number}`;
  if (typeof item.repository_version === "string" && !item.name) {
    const n = versionNumber(item.repository_version);
    const when = typeof item.pulp_created === "string" ? ` · ${relativeTime(item.pulp_created)}` : "";
    return `${repoName || "Publication"}${n !== null ? ` · version ${n}` : ""}${when}`;
  }
  return displayName(item, href);
}

export function HrefPicker({
  type,
  value,
  onChange,
  label,
  invalid,
  labelId,
  types: explicitTypes,
  listPath,
  params,
}: {
  type: ResourceType;
  /** Types searched together (default: `type` and its variants, e.g. file and git remotes). */
  types?: ResourceType[];
  /** List to search instead of the type's (a repository's `versions/`). */
  listPath?: string;
  /** Extra filters (`repository=` for publications). */
  params?: Record<string, string>;
  value: string;
  onChange: (href: string) => void;
  label?: string;
  invalid?: boolean;
  /** Id of an external label (form rows). */
  labelId?: string;
}) {
  const {pulp, ctx, registry} = useSession();
  const inputId = useId();
  const anchor = useRef<HTMLDivElement>(null);
  const [input, setInput] = useState("");
  const [open, setOpen] = useState(false);
  const known = useQuery({
    queryKey: ["pulp", ctx, "pick-name", value],
    queryFn: () => pulp.get(value, {fields: "pulp_href,name,base_path,number,username,repository,repository_version,pulp_created"}),
    enabled: value.startsWith("/"),
    staleTime: 60_000,
  });
  const knownRepo = useRepoNames(known.data ? [known.data] : []);
  const knownName = known.data ? describe(known.data, value, knownRepo, !!listPath) : "";
  useEffect(() => {
    if (knownName) setInput(knownName);
  }, [knownName]);
  const term = input && input !== knownName && !input.startsWith("/") ? input : "";
  // Every variant of the type (file and git remotes…), searched together.
  const types = listPath ? [type] : (explicitTypes ?? registry.variants(type)).filter((t) => t.list_path);
  const results = useQueries({
    queries: types.map((t) => {
      const search = t.list_params.some((p) => p.name === "name__icontains") ? "name__icontains" : null;
      const ordering = t.list_params.find((p) => p.name === "ordering")?.enum_values ?? [];
      return {
        queryKey: ["pulp", ctx, "picker", listPath ?? t.id, params, term],
        queryFn: () =>
          pulp.list(listPath ?? t.list_path!, {
            limit: 25,
            ordering: listPath ? "-number" : ordering.includes("-pulp_created") ? "-pulp_created" : undefined,
            ...params,
            ...(search && term ? {[search]: term} : {}),
          }),
        enabled: open,
        staleTime: 10_000,
      };
    }),
  });
  const fetching = results.some((r) => r.isFetching);
  const loading = results.some((r) => r.isLoading);
  const items = results.flatMap((r, i) => (r.data?.results ?? []).map((item) => ({item, type: types[i]})));
  const repoNames = useRepoNames(items.map((i) => i.item));
  const variantLabel = (t: ResourceType) => (t === type ? null : titleize(t.name));

  const picker = (
    <CommandPrimitive className="w-full" shouldFilter={false} onKeyDown={(e) => e.key === "Escape" && open && e.stopPropagation()}>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverAnchor asChild>
          <InputGroup ref={anchor}>
            <CommandPrimitive.Input
              aria-invalid={invalid || undefined}
              aria-label={label || labelId ? undefined : typeLabel(type)}
              aria-labelledby={labelId}
              className="h-full min-w-0 flex-1 bg-transparent px-2.5 text-sm outline-none placeholder:text-muted-foreground"
              data-slot="input-group-control"
              id={inputId}
              placeholder={`Search ${(types.length > 1 ? categoryLabel(type.category) : typeLabel(type)).toLowerCase()}…`}
              value={input}
              onClick={() => setOpen(true)}
              onFocus={() => setOpen(true)}
              onValueChange={(text) => {
                setInput(text);
                setOpen(true);
                if (!text) onChange("");
                else if (text.startsWith("/")) onChange(text);
              }}
            />
            <InputGroupAddon align="inline-end">
              {fetching ? <Spinner /> : <ChevronsUpDown className="opacity-50" />}
            </InputGroupAddon>
          </InputGroup>
        </PopoverAnchor>
        <PopoverContent
          align="start"
          className="w-(--radix-popover-trigger-width) p-1"
          onInteractOutside={(e) => {
            if (anchor.current?.contains(e.target as Node)) e.preventDefault();
          }}
          onOpenAutoFocus={(e) => e.preventDefault()}
        >
          <CommandList>
            {!loading && <CommandEmpty>No match</CommandEmpty>}
            {items.map(({item, type: t}) => {
              const href = item.pulp_href as string;
              const variant = variantLabel(t);
              return (
                <CommandItem
                  key={href}
                  data-checked={href === value}
                  value={href}
                  onSelect={() => {
                    onChange(href);
                    setInput(describe(item, href, repoNames, !!listPath));
                    setOpen(false);
                  }}
                >
                  <span className="truncate">{describe(item, href, repoNames, !!listPath)}</span>
                  {variant && <span className="ms-auto shrink-0 text-xs text-muted-foreground">{variant}</span>}
                </CommandItem>
              );
            })}
          </CommandList>
        </PopoverContent>
      </Popover>
    </CommandPrimitive>
  );
  if (!label) return picker;
  return (
    <Field>
      <FieldLabel htmlFor={inputId}>{label}</FieldLabel>
      {picker}
    </Field>
  );
}
