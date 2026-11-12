// Search-as-you-type picker returning a resource href (pasting an href also works).

import {ComboBox, Input, Label, ListBox} from "@heroui/react";
import {useQuery} from "@tanstack/react-query";
import {useEffect, useState} from "react";

import {displayName, typeLabel} from "../lib/format";
import type {ResourceType} from "../lib/registry";
import {useSession} from "../lib/session";

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
  const [input, setInput] = useState("");
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
    enabled: !!type.list_path,
    staleTime: 10_000,
  });
  const items = options.data?.results ?? [];
  return (
    <ComboBox
      allowsCustomValue
      fullWidth
      aria-label={label || labelId ? undefined : typeLabel(type)}
      aria-labelledby={labelId}
      inputValue={input}
      isInvalid={invalid}
      menuTrigger="focus"
      selectedKey={value || null}
      validationBehavior="aria"
      onInputChange={(text) => {
        setInput(text);
        if (!text) onChange("");
        else if (text.startsWith("/")) onChange(text);
      }}
      onSelectionChange={(key) => {
        if (key === null) return;
        const href = String(key);
        onChange(href);
        const item = items.find((i) => i.pulp_href === href);
        if (item) setInput(displayName(item, href));
      }}
    >
      {label && <Label>{label}</Label>}
      <ComboBox.InputGroup>
        <Input placeholder={`Search ${typeLabel(type).toLowerCase()}…`} />
        <ComboBox.Trigger />
      </ComboBox.InputGroup>
      <ComboBox.Popover>
        <ListBox>
          {items.map((item) => (
            <ListBox.Item key={item.pulp_href as string} id={item.pulp_href as string} textValue={displayName(item)}>
              {displayName(item)}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </ComboBox.Popover>
    </ComboBox>
  );
}
