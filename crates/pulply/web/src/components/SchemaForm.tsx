// Forms generated from OpenAPI request schemas: creation, edition (PATCH of the changed
// fields only) and actions/operations. Files go out as multipart parts.

import {
  Button,
  Description,
  Disclosure,
  Input,
  Label,
  ListBox,
  Modal,
  NumberField,
  Select,
  Spinner,
  Switch,
  TextArea,
  TextField,
  toast,
} from "@heroui/react";
import {useQueryClient} from "@tanstack/react-query";
import {type ReactNode, useEffect, useId, useMemo, useState} from "react";

import {ApiError, type Body, type Json, taskOf} from "../lib/api";
import {fieldLabel} from "../lib/format";
import type {Property, Registry, Schema} from "../lib/registry";
import {errorText, usePulp, useSession} from "../lib/session";
import {ResultDialog} from "./dialogs";
import {HrefPicker} from "./HrefPicker";

export type Kind =
  | {k: "text"}
  | {k: "secret"}
  | {k: "integer"}
  | {k: "number"}
  | {k: "bool"}
  | {k: "enum"; choices: string[]}
  | {k: "list"; choices: string[]}
  | {k: "json"}
  | {k: "relation"; target: string | null}
  | {k: "file"};

export interface FieldDef {
  name: string;
  kind: Kind;
  required: boolean;
  nullable: boolean;
  help: string | null;
  placeholder: string | null;
}

/** Fields shown before "Advanced options". */
const PRIMARY = new Set([
  "name",
  "url",
  "policy",
  "description",
  "base_path",
  "repository",
  "repository_version",
  "publication",
  "remote",
  "content_guard",
  "pulp_labels",
  "relative_path",
  "file",
  "upload",
  "artifact",
  "username",
  "password",
  "email",
  "first_name",
  "last_name",
  "is_staff",
  "is_active",
  "role",
  "users",
  "groups",
  "content_object",
  "mirror",
  "sync_policy",
  "repo_hrefs",
  "keep_versions",
  "base_version",
  "add_content_units",
  "remove_content_units",
  "config",
  "path",
  "toc",
  "dry_run",
  "keep_days",
  "finished_before",
  "states",
  "orphan_protection_time",
  "content_hrefs",
  "storage_class",
  "storage_settings",
  "replace",
  "permissions",
  "label",
  "key",
  "value",
]);

function textOf(v: unknown, kind: Kind): string {
  if (v === null || v === undefined) return "";
  if (typeof v === "string") return v;
  if (typeof v === "boolean" || typeof v === "number") return String(v);
  if (Array.isArray(v) && kind.k === "list" && v.every((x) => typeof x === "string")) return v.join(", ");
  return JSON.stringify(v, null, 2);
}

/** Picker target for an href field, by Pulp serializer naming conventions. */
function relationTarget(field: string, plugin: string, reg: Registry): string | null {
  const byCategory: Record<string, string> = {remote: "remotes", repository: "repositories", publication: "publications"};
  if (field === "content_guard") return reg.get("contentguards") ? "contentguards" : null;
  if (field === "repository_version" || field === "base_version") {
    return reg.get("repository_versions") ? "repository_versions" : null;
  }
  if (field.endsWith("signing_service")) return reg.get("signing-services") ? "signing-services" : null;
  const category = byCategory[field];
  if (!category) return null;
  return reg.pluginType(category, plugin, false)?.id ?? (reg.get(category) ? category : null);
}

export function fieldsFromSchema(schema: Schema | undefined, reg: Registry, plugin: string): FieldDef[] {
  const props = Object.entries(schema?.properties ?? {}).filter(([, p]) => !p.readOnly);
  const fields = props.map(([name, p]) => buildField(name, p, schema!, reg, plugin));
  return fields.sort((a, b) => Number(b.required) - Number(a.required));
}

function buildField(name: string, p: Property, schema: Schema, reg: Registry, plugin: string): FieldDef {
  const choices = reg.enumChoices(p);
  let kind: Kind;
  if (p.format === "binary") kind = {k: "file"};
  else if (p.type === "array") kind = {k: "list", choices};
  else if (choices.length) kind = {k: "enum", choices};
  else if (p.type === "boolean") kind = {k: "bool"};
  else if (p.type === "integer") kind = {k: "integer"};
  else if (p.type === "number") kind = {k: "number"};
  else if (p.type === "object") kind = {k: "json"};
  else if (p.format === "uri") kind = {k: "relation", target: relationTarget(name, plugin, reg)};
  else if (p.writeOnly) kind = {k: "secret"};
  else kind = {k: "text"};
  const help = p.description ? p.description.split("\n\n")[0].replace(/\n/g, " ") : null;
  const placeholder = p.default !== undefined && p.default !== null ? textOf(p.default, kind) || null : null;
  return {name, kind, required: !!schema.required?.includes(name), nullable: !!p.nullable, help, placeholder};
}

/** Typed value of a field's text; `undefined` when empty. Throws a message on invalid input. */
function parse(f: FieldDef, raw: string): unknown {
  const text = raw.trim();
  if (!text) return undefined;
  switch (f.kind.k) {
    case "integer": {
      const n = Number(text);
      if (!Number.isInteger(n)) throw new Error("Not an integer");
      return n;
    }
    case "number": {
      const n = Number(text);
      if (Number.isNaN(n)) throw new Error("Not a number");
      return n;
    }
    case "bool":
      return text === "true";
    case "list":
      return text
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
    case "json":
      try {
        return JSON.parse(text);
      } catch (e) {
        throw new Error(`Invalid JSON: ${(e as Error).message}`);
      }
    default:
      return text;
  }
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

export interface FormState {
  values: Record<string, string>;
  files: Record<string, File | null>;
}

/** Request body from the inputs, or field errors. `original` (edit) keeps only changes. */
export function buildBody(
  fields: FieldDef[],
  state: FormState,
  original: Json | null,
): {value: Json; files: Record<string, File>} | {errors: Record<string, string[]>} {
  const value: Json = {};
  const files: Record<string, File> = {};
  const errors: Record<string, string[]> = {};
  for (const f of fields) {
    if (f.kind.k === "file") {
      const file = state.files[f.name];
      if (file) files[f.name] = file;
      else if (f.required && !original) errors[f.name] = ["Required"];
      continue;
    }
    let v: unknown;
    try {
      v = parse(f, state.values[f.name] ?? "");
    } catch (e) {
      errors[f.name] = [(e as Error).message];
      continue;
    }
    if (original) {
      const old = original[f.name];
      let normalized: unknown;
      try {
        normalized = old === null || old === undefined ? undefined : parse(f, textOf(old, f.kind));
      } catch {
        normalized = old;
      }
      if (same(v, normalized)) continue;
      if (v !== undefined) value[f.name] = v;
      else if (f.nullable) value[f.name] = null;
    } else if (v !== undefined) value[f.name] = v;
    else if (f.required) errors[f.name] = ["Required"];
  }
  return Object.keys(errors).length ? {errors} : {value, files};
}

// ---------------------------------------------------------------- inputs

function RelationInput({field, value, onChange, invalid, labelId}: FieldProps) {
  const {registry} = useSession();
  const target = field.kind.k === "relation" && field.kind.target ? registry.get(field.kind.target) : undefined;
  if (!target?.list_path) {
    return (
      <TextField fullWidth aria-labelledby={labelId} isInvalid={invalid} validationBehavior="aria" value={value} onChange={onChange}>
        <Input placeholder="/pulp/api/v3/…" />
      </TextField>
    );
  }
  return <HrefPicker invalid={invalid} labelId={labelId} type={target} value={value} onChange={onChange} />;
}

interface FieldProps {
  field: FieldDef;
  value: string;
  onChange: (v: string) => void;
  invalid: boolean;
  /** Id of the visible label. */
  labelId: string;
}

function FieldInput(props: FieldProps & {file: File | null; onFile: (f: File | null) => void}) {
  const {field, value, onChange, invalid} = props;
  const kind = field.kind;
  switch (kind.k) {
    case "bool": {
      const unset = value === "";
      return (
        <div className="flex items-center gap-3">
          <Switch aria-labelledby={props.labelId} isSelected={value === "true"} onChange={(on) => onChange(on ? "true" : "false")}>
            <Switch.Content>
              <Switch.Control>
                <Switch.Thumb />
              </Switch.Control>
              <span className="text-sm">{unset ? `Default${field.placeholder ? ` (${field.placeholder})` : ""}` : value === "true" ? "Yes" : "No"}</span>
            </Switch.Content>
          </Switch>
          {!unset && !field.required && (
            <Button size="sm" variant="ghost" onPress={() => onChange("")}>
              Reset
            </Button>
          )}
        </div>
      );
    }
    case "enum":
      return (
        <Select
          fullWidth
          aria-labelledby={props.labelId}
          isInvalid={invalid}
          placeholder={field.placeholder ? `Default (${field.placeholder})` : "Not set"}
          validationBehavior="aria"
          value={value || null}
          onChange={(key) => onChange(key === null || key === "__unset" ? "" : String(key))}
        >
          <Select.Trigger>
            <Select.Value />
            <Select.Indicator />
          </Select.Trigger>
          <Select.Popover>
            <ListBox>
              {[
                ...(field.required ? [] : [{id: "__unset", label: "Not set"}]),
                ...kind.choices.map((c) => ({id: c, label: c})),
              ].map((c) => (
                <ListBox.Item key={c.id} id={c.id} textValue={c.label}>
                  {c.label}
                  <ListBox.ItemIndicator />
                </ListBox.Item>
              ))}
            </ListBox>
          </Select.Popover>
        </Select>
      );
    case "list":
      if (kind.choices.length) {
        const selected = value
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean);
        return (
          <Select
            fullWidth
            aria-labelledby={props.labelId}
            placeholder="None"
            selectionMode="multiple"
            value={selected}
            onChange={(keys) => onChange(((keys as unknown[]) ?? []).map(String).join(", "))}
          >
            <Select.Trigger>
              <Select.Value />
              <Select.Indicator />
            </Select.Trigger>
            <Select.Popover>
              <ListBox selectionMode="multiple">
                {kind.choices.map((c) => (
                  <ListBox.Item key={c} id={c} textValue={c}>
                    {c}
                    <ListBox.ItemIndicator />
                  </ListBox.Item>
                ))}
              </ListBox>
            </Select.Popover>
          </Select>
        );
      }
      return (
        <TextField fullWidth aria-labelledby={props.labelId} isInvalid={invalid} validationBehavior="aria" value={value} onChange={onChange}>
          <Input placeholder="Comma-separated values" />
        </TextField>
      );
    case "json":
      return (
        <TextField fullWidth aria-labelledby={props.labelId} isInvalid={invalid} validationBehavior="aria" value={value} onChange={onChange}>
          <TextArea className="font-mono text-xs" placeholder={field.placeholder ?? '{"key": "value"}'} rows={3} />
        </TextField>
      );
    case "integer":
    case "number":
      return (
        <NumberField
          fullWidth
          aria-labelledby={props.labelId}
          isInvalid={invalid}
          step={kind.k === "integer" ? 1 : 0.01}
          validationBehavior="aria"
          value={value === "" ? Number.NaN : Number(value)}
          onChange={(n) => onChange(n === undefined || Number.isNaN(n) ? "" : String(n))}
        >
          <NumberField.Group>
            <NumberField.DecrementButton />
            <NumberField.Input placeholder={field.placeholder ?? undefined} />
            <NumberField.IncrementButton />
          </NumberField.Group>
        </NumberField>
      );
    case "relation":
      return <RelationInput {...props} />;
    case "file":
      return (
        <label className="flex items-center gap-3 rounded-xl border border-dashed border-default p-3 cursor-pointer hover:bg-default/40">
          <input
            aria-labelledby={props.labelId}
            className="hidden"
            type="file"
            onChange={(e) => props.onFile(e.target.files?.[0] ?? null)}
          />
          <span className="text-sm font-medium text-accent">Choose a file</span>
          <span className="text-sm text-muted truncate">{props.file ? props.file.name : "No file selected"}</span>
        </label>
      );
    default:
      return (
        <TextField
          fullWidth
          aria-labelledby={props.labelId}
          isInvalid={invalid}
          type={kind.k === "secret" ? "password" : "text"}
          validationBehavior="aria"
          value={value}
          onChange={onChange}
        >
          <Input placeholder={field.placeholder ?? undefined} />
        </TextField>
      );
  }
}

function FieldRow({
  field,
  state,
  setState,
  errors,
}: {
  field: FieldDef;
  state: FormState;
  setState: (fn: (s: FormState) => FormState) => void;
  errors: Record<string, string[]>;
}) {
  const err = errors[field.name];
  const labelId = useId();
  return (
    <div className="flex flex-col gap-1.5">
      <Label id={labelId} isRequired={field.required}>
        {fieldLabel(field.name)}
      </Label>
      <FieldInput
        field={field}
        labelId={labelId}
        file={state.files[field.name] ?? null}
        invalid={!!err}
        value={state.values[field.name] ?? ""}
        onChange={(v) => setState((s) => ({...s, values: {...s.values, [field.name]: v}}))}
        onFile={(f) => setState((s) => ({...s, files: {...s.files, [field.name]: f}}))}
      />
      {err ? (
        <span className="text-danger text-xs">{err.join(" ")}</span>
      ) : field.help ? (
        <Description className="text-xs text-muted">{field.help}</Description>
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------- form modal

export interface FormSpec {
  title: string;
  /** Shown in task notifications (`Sync iso`). */
  label: string;
  method: "POST" | "PATCH";
  path: string;
  schema: Schema | undefined;
  plugin: string;
  multipart?: boolean;
  /** Pre-filled values (field → API value). */
  initial?: Json;
  /** Edit: the resource to load and compare against. */
  editHref?: string;
  submitLabel?: string;
  intro?: ReactNode;
  /** Ask for confirmation before sending (destructive operations). */
  danger?: boolean;
  /** Display a synchronous (non-task) JSON response. */
  showResult?: boolean;
  onDone?: (response: unknown) => void;
}

export function FormModal({spec, onClose}: {spec: FormSpec | null; onClose: () => void}) {
  return (
    <Modal.Backdrop isOpen={!!spec} onOpenChange={(open) => !open && onClose()}>
      <Modal.Container scroll="inside" size="lg">
        <Modal.Dialog>{spec && <FormBody key={spec.path + spec.title} spec={spec} onClose={onClose} />}</Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}

function FormBody({spec, onClose}: {spec: FormSpec; onClose: () => void}) {
  const {registry, pulp, track, ctx} = useSession();
  const queryClient = useQueryClient();
  const fields = useMemo(() => fieldsFromSchema(spec.schema, registry, spec.plugin), [spec, registry]);
  const loaded = usePulp(spec.editHref);
  const original = spec.editHref ? (loaded.data ?? null) : null;
  const [state, setState] = useState<FormState>(() => ({
    values: Object.fromEntries(fields.map((f) => [f.name, textOf(spec.initial?.[f.name], f.kind)])),
    files: {},
  }));
  useEffect(() => {
    if (original) {
      setState((s) => ({...s, values: Object.fromEntries(fields.map((f) => [f.name, textOf(original[f.name], f.kind)]))}));
    }
  }, [original, fields]);
  const [errors, setErrors] = useState<Record<string, string[]>>({});
  const [general, setGeneral] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [result, setResult] = useState<unknown | null>(null);

  const primary = fields.filter((f) => f.required || PRIMARY.has(f.name) || (spec.initial && f.name in spec.initial));
  const advanced = fields.filter((f) => !primary.includes(f));

  const submit = async () => {
    const built = buildBody(fields, state, original);
    if ("errors" in built) {
      setErrors(built.errors);
      return;
    }
    if (original && !Object.keys(built.value).length && !Object.keys(built.files).length) {
      toast.info("No changes");
      onClose();
      return;
    }
    if (spec.danger && !confirming) {
      setConfirming(true);
      return;
    }
    setPending(true);
    setErrors({});
    setGeneral(null);
    try {
      const body: Body = {value: built.value, files: built.files, multipart: spec.multipart};
      const response = await pulp.send(spec.method, spec.path, body);
      queryClient.invalidateQueries({queryKey: ["pulp", ctx]});
      spec.onDone?.(response);
      if (track(response, spec.label)) onClose();
      else if (spec.showResult && response && typeof response === "object" && !taskOf(response) && !("task_group" in (response as Json))) {
        setResult(response);
      } else {
        toast.success(`${spec.label} done`);
        onClose();
      }
    } catch (e) {
      setConfirming(false);
      if (e instanceof ApiError && e.status === 400) {
        const known = Object.fromEntries(Object.entries(e.fields).filter(([k]) => fields.some((f) => f.name === k)));
        const rest = Object.entries(e.fields).filter(([k]) => !(k in known));
        setErrors(known);
        setGeneral(rest.map(([k, v]) => (k === "non_field_errors" || k === "detail" ? v.join(" ") : `${k}: ${v.join(" ")}`)).join("; ") || null);
      } else setGeneral(errorText(e));
    } finally {
      setPending(false);
    }
  };

  return (
    <>
      <Modal.CloseTrigger />
      <Modal.Header>
        <Modal.Heading>{spec.title}</Modal.Heading>
      </Modal.Header>
      <Modal.Body>
        <form
          className="flex flex-col gap-4 py-1"
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          {spec.intro}
          {spec.editHref && loaded.isLoading && <Spinner />}
          {!fields.length && <p className="text-muted text-sm">This action takes no parameters.</p>}
          {primary.map((f) => (
            <FieldRow key={f.name} errors={errors} field={f} setState={setState} state={state} />
          ))}
          {advanced.length > 0 && (
            <Disclosure defaultExpanded={advanced.some((f) => errors[f.name])}>
              <Disclosure.Heading>
                <Button className="w-full justify-between" slot="trigger" variant="ghost">
                  Advanced options ({advanced.length})
                  <Disclosure.Indicator />
                </Button>
              </Disclosure.Heading>
              <Disclosure.Content>
                <Disclosure.Body className="flex flex-col gap-4 pt-3">
                  {advanced.map((f) => (
                    <FieldRow key={f.name} errors={errors} field={f} setState={setState} state={state} />
                  ))}
                </Disclosure.Body>
              </Disclosure.Content>
            </Disclosure>
          )}
          {general && <p className="text-danger text-sm">{general}</p>}
          {confirming && (
            <p className="text-warning text-sm font-medium">This operation is destructive. Press the button again to confirm.</p>
          )}
          <button className="hidden" type="submit" />
        </form>
      </Modal.Body>
      <Modal.Footer>
        <Button slot="close" variant="tertiary">
          Cancel
        </Button>
        <Button isPending={pending} variant={spec.danger ? "danger" : "primary"} onPress={() => void submit()}>
          {pending && <Spinner color="current" size="sm" />}
          {confirming ? "Confirm" : (spec.submitLabel ?? (spec.method === "PATCH" ? "Save" : "Submit"))}
        </Button>
      </Modal.Footer>
      <ResultDialog
        title={spec.title}
        value={result}
        onClose={() => {
          setResult(null);
          onClose();
        }}
      />
    </>
  );
}
