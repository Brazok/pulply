// Search-as-you-type picker returning a resource href (pasting an href also works).

import {useQuery} from "@tanstack/react-query";
import {Command as CommandPrimitive} from "cmdk";
import {ChevronsUpDown} from "lucide-react";
import {useEffect, useId, useRef, useState} from "react";

import {displayName, typeLabel} from "../lib/format";
import type {ResourceType} from "../lib/registry";
import {useSession} from "../lib/session";
import {CommandEmpty, CommandItem, CommandList} from "./ui/command";
import {Field, FieldLabel} from "./ui/field";
import {InputGroup, InputGroupAddon} from "./ui/input-group";
import {Popover, PopoverAnchor, PopoverContent} from "./ui/popover";
import {Spinner} from "./ui/spinner";

export function HrefPicker({
  type,
  value,
  onChange,
  label,
  invalid,
  labelId,
}: {
  type: ResourceType;
  value: string;
  onChange: (href: string) => void;
  label?: string;
  invalid?: boolean;
  /** Id of an external label (form rows). */
  labelId?: string;
}) {
  const {pulp, ctx} = useSession();
  const inputId = useId();
  const anchor = useRef<HTMLDivElement>(null);
  const [input, setInput] = useState("");
  const [open, setOpen] = useState(false);
  const known = useQuery({
    queryKey: ["pulp", ctx, "pick-name", value],
    queryFn: () => pulp.get(value, {fields: "pulp_href,name,base_path,number,username"}),
    enabled: value.startsWith("/"),
    staleTime: 60_000,
  });
  const knownName = known.data ? displayName(known.data, value) : "";
  useEffect(() => {
    if (knownName) setInput(knownName);
  }, [knownName]);
  const search = type.list_params.some((p) => p.name === "name__icontains") ? "name__icontains" : null;
  const term = input && input !== knownName && !input.startsWith("/") ? input : "";
  const ordering = type.list_params.find((p) => p.name === "ordering")?.enum_values ?? [];
  const options = useQuery({
    queryKey: ["pulp", ctx, "picker", type.id, term],
    queryFn: () =>
      pulp.list(type.list_path!, {
        limit: 25,
        ordering: ordering.includes("-pulp_created") ? "-pulp_created" : undefined,
        ...(search && term ? {[search]: term} : {}),
      }),
    enabled: !!type.list_path && open,
    staleTime: 10_000,
  });
  const items = options.data?.results ?? [];

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
              placeholder={`Search ${typeLabel(type).toLowerCase()}…`}
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
              {options.isFetching ? <Spinner /> : <ChevronsUpDown className="opacity-50" />}
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
            {!options.isLoading && <CommandEmpty>No match</CommandEmpty>}
            {items.map((item) => {
              const href = item.pulp_href as string;
              return (
                <CommandItem
                  key={href}
                  data-checked={href === value}
                  value={href}
                  onSelect={() => {
                    onChange(href);
                    setInput(displayName(item, href));
                    setOpen(false);
                  }}
                >
                  <span className="truncate">{displayName(item)}</span>
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
