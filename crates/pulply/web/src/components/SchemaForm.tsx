// Forms generated from OpenAPI request schemas: creation, edition (PATCH of the changed
// fields only) and actions/operations. Files go out as multipart parts.

import {useQueryClient} from "@tanstack/react-query";
import {ChevronDown} from "lucide-react";
import {type ReactNode, createContext, useContext, useEffect, useId, useMemo, useState} from "react";
import {Link} from "react-router";
import {toast} from "sonner";

import {ApiError, type Body, type Json, taskOf} from "../lib/api";
import {fieldLabel} from "../lib/format";
import {type Property, type Registry, type ResourceType, type Schema, versionRepository} from "../lib/registry";
import {listUrl} from "../lib/routes";
import {errorText, usePulp, useSession} from "../lib/session";
import {cn} from "../lib/utils";
import {ResultDialog} from "./dialogs";
import {HrefPicker} from "./HrefPicker";
import {Button} from "./ui/button";
import {Collapsible, CollapsibleContent, CollapsibleTrigger} from "./ui/collapsible";
import {Dialog, DialogClose, DialogContent, DialogFooter, DialogHeader, DialogTitle} from "./ui/dialog";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "./ui/dropdown-menu";
import {Field, FieldDescription, FieldError, FieldLabel} from "./ui/field";
import {Input} from "./ui/input";
import {Select, SelectContent, SelectItem, SelectTrigger, SelectValue} from "./ui/select";
import {Spinner} from "./ui/spinner";
import {Switch} from "./ui/switch";
import {Textarea} from "./ui/textarea";

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

/** Radix Select items cannot carry an empty value. */
const UNSET = "__unset";

/** What a relation picker knows about its form: the other values and the resource concerned. */
interface FormContextValue {
  values: Record<string, string>;
  /** Type of the resource created, edited or acted upon. */
  type?: ResourceType;
  /** Repository the form is about (its own form, or one of its actions). */
  repository?: string;
}

const FormContext = createContext<FormContextValue>({values: {}});

function RelationInput({field, value, onChange, invalid, labelId}: FieldProps) {
  const {registry, ctx} = useSession();
  const form = useContext(FormContext);
  const target = field.kind.k === "relation" && field.kind.target ? registry.get(field.kind.target) : undefined;
  const repo = form.values.repository?.startsWith("/")
    ? form.values.repository
    : form.values[SOURCE_REPO]?.startsWith("/")
      ? form.values[SOURCE_REPO]
      : form.repository;
  const repoType = repo ? registry.typeForHref(repo) : undefined;

  // Versions of the form's repository only (otherwise every repository's "Version 1"…).
  if ((field.name === "repository_version" || field.name === "base_version") && repo && repoType) {
    const versions = registry.get(`${repoType.id}/versions`);
    if (versions) {
      return <HrefPicker invalid={invalid} labelId={labelId} listPath={`${repo}versions/`} type={versions} value={value} onChange={onChange} />;
    }
  }
  if (!target?.list_path) {
    return (
      <Input
        aria-invalid={invalid || undefined}
        aria-labelledby={labelId}
        placeholder="/pulp/api/v3/…"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    );
  }
  let types = registry.variants(target);
  // A pull-through remote only serves a pull-through distribution, never a repository.
  let hidden: ResourceType | undefined;
  if (field.name === "remote") {
    const pullThrough = form.type?.name === "pull-through";
    const kept = types.filter((t) => (t.name === "pull-through") === pullThrough);
    const all = pullThrough ? registry.types.filter((t) => t.category === "remotes" && t.plugin === target.plugin && t.name === "pull-through") : kept;
    if (!pullThrough) hidden = types.find((t) => t.name === "pull-through");
    if (all.length) types = all;
  }
  const params = field.name === "publication" && repo && target.list_params.some((p) => p.name === "repository") ? {repository: repo} : undefined;
  const picker = <HrefPicker invalid={invalid} labelId={labelId} params={params} type={types[0]} types={types} value={value} onChange={onChange} />;
  if (!hidden) return picker;
  const dist = registry.types.find((t) => t.category === "distributions" && t.plugin === hidden.plugin && t.name === "pull-through");
  return (
    <div className="flex flex-col gap-1.5">
      {picker}
      <p className="text-xs text-muted-foreground">
        Pull-through remotes are not listed: Pulp only accepts them on a{" "}
        {dist ? (
          <Link className="text-primary hover:underline" to={listUrl(ctx, dist.id)}>
            pull-through distribution
          </Link>
        ) : (
          "pull-through distribution"
        )}
        , which caches the images pulled through it and creates their repositories by itself. Open the pull-through remote and use{" "}
        <b>New pull-through distribution</b>.
      </p>
    </div>
  );
}

interface FieldProps {
  field: FieldDef;
  value: string;
  onChange: (v: string) => void;
  invalid: boolean;
  /** Id of the visible label. */
  labelId: string;
}

function MultiSelect({choices, value, onChange, labelId}: {choices: string[]; value: string; onChange: (v: string) => void; labelId: string}) {
  const selected = value
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  const set = (c: string, on: boolean) => onChange((on ? [...selected, c] : selected.filter((x) => x !== c)).join(", "));
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button aria-labelledby={labelId} className="w-full justify-between font-normal" variant="outline">
          <span className={cn("truncate", !selected.length && "text-muted-foreground")}>{selected.length ? selected.join(", ") : "None"}</span>
          <ChevronDown className="text-muted-foreground" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-(--radix-dropdown-menu-trigger-width)">
        {choices.map((c) => (
          <DropdownMenuCheckboxItem
            key={c}
            checked={selected.includes(c)}
            onCheckedChange={(on) => set(c, on === true)}
            onSelect={(e) => e.preventDefault()}
          >
            {c}
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function FieldInput(props: FieldProps & {file: File | null; onFile: (f: File | null) => void}) {
  const {field, value, onChange, invalid, labelId} = props;
  const kind = field.kind;
  const common = {"aria-invalid": invalid || undefined, "aria-labelledby": labelId};
  switch (kind.k) {
    case "bool": {
      const unset = value === "";
      return (
        <div className="flex items-center gap-3">
          <Switch aria-labelledby={labelId} checked={value === "true"} onCheckedChange={(on) => onChange(on ? "true" : "false")} />
          <span className="text-sm">{unset ? `Default${field.placeholder ? ` (${field.placeholder})` : ""}` : value === "true" ? "Yes" : "No"}</span>
          {!unset && !field.required && (
            <Button size="sm" variant="ghost" onClick={() => onChange("")}>
              Reset
            </Button>
          )}
        </div>
      );
    }
    case "enum":
      return (
        <Select value={value || (field.required ? "" : UNSET)} onValueChange={(v) => onChange(v === UNSET ? "" : v)}>
          <SelectTrigger {...common} className="w-full">
            <SelectValue placeholder={field.placeholder ? `Default (${field.placeholder})` : "Not set"} />
          </SelectTrigger>
          <SelectContent>
            {!field.required && (
              <SelectItem value={UNSET}>
                <span className="text-muted-foreground">{field.placeholder ? `Default (${field.placeholder})` : "Not set"}</span>
              </SelectItem>
            )}
            {kind.choices.map((c) => (
              <SelectItem key={c} value={c}>
                {c}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      );
    case "list":
      if (kind.choices.length) return <MultiSelect choices={kind.choices} labelId={labelId} value={value} onChange={onChange} />;
      return <Input {...common} placeholder="Comma-separated values" value={value} onChange={(e) => onChange(e.target.value)} />;
    case "json":
      return (
        <Textarea
          {...common}
          className="font-mono text-xs"
          placeholder={field.placeholder ?? '{"key": "value"}'}
          rows={3}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "integer":
    case "number":
      return (
        <Input
          {...common}
          inputMode={kind.k === "integer" ? "numeric" : "decimal"}
          placeholder={field.placeholder ?? undefined}
          step={kind.k === "integer" ? 1 : "any"}
          type="number"
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "relation":
      return <RelationInput {...props} />;
    case "file":
      return (
        <label className="flex cursor-pointer items-center gap-3 rounded-lg border border-dashed p-3 transition-colors hover:bg-muted/50">
          <input aria-labelledby={labelId} className="hidden" type="file" onChange={(e) => props.onFile(e.target.files?.[0] ?? null)} />
          <span className="text-sm font-medium text-primary">Choose a file</span>
          <span className="truncate text-sm text-muted-foreground">{props.file ? props.file.name : "No file selected"}</span>
        </label>
      );
    default:
      return (
        <Input
          {...common}
          placeholder={field.placeholder ?? undefined}
          type={kind.k === "secret" ? "password" : "text"}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      );
  }
}

// ---------------------------------------------------------------- distribution source

/** Fields of a distribution naming what it serves: at most one may be set. */
const SOURCES = ["repository", "repository_version", "publication"] as const;
type Source = (typeof SOURCES)[number] | "none";
/** Helper value (never sent): repository whose versions or publications are offered. */
const SOURCE_REPO = "__source_repository";

const SOURCE_TEXT: Record<Source, {title: string; text: string}> = {
  repository: {title: "Latest version of a repository", text: "Follows the repository: each new version (sync, upload…) is served automatically."},
  repository_version: {title: "A fixed repository version", text: "Frozen on the version you pick, whatever happens to the repository afterwards."},
  publication: {title: "A publication", text: "Serves a publication you created, with its metadata, until you pick another one."},
  none: {title: "Nothing for now", text: "The distribution exists, but serves no content yet."},
};

export function sourceOf(values: Record<string, string>): Source | null {
  return SOURCES.find((f) => values[f]?.trim()) ?? null;
}

function SourceChooser({
  fields,
  plugin,
  state,
  setState,
  errors,
}: {
  fields: FieldDef[];
  plugin: string;
  state: FormState;
  setState: (fn: (s: FormState) => FormState) => void;
  errors: Record<string, string[]>;
}) {
  const {registry} = useSession();
  const available = SOURCES.filter((f) => fields.some((x) => x.name === f));
  const [mode, setMode] = useState<Source>(() => sourceOf(state.values) ?? (available.includes("repository") ? "repository" : available[0]));
  // The edited distribution loads after the first render.
  const loaded = sourceOf(state.values);
  useEffect(() => {
    if (loaded) setMode(loaded);
  }, [loaded]);
  const repoType = registry.pluginType("repositories", plugin, false);
  const choose = (next: Source) => {
    setMode(next);
    setState((s) => ({...s, values: {...s.values, ...Object.fromEntries(SOURCES.filter((f) => f !== next).map((f) => [f, ""]))}}));
  };
  const field = fields.find((f) => f.name === mode);
  const helperRepo = state.values[SOURCE_REPO] || (mode === "repository_version" && state.values.repository_version ? (versionRepository(state.values.repository_version) ?? "") : "");
  const options: Source[] = [...available, ...(fields.every((f) => !f.required) ? (["none"] as const) : [])];
  const groupId = useId();
  return (
    <div className="flex flex-col gap-3 rounded-xl border bg-muted/30 p-4">
      <div className="flex flex-col gap-0.5">
        <span className="text-sm font-medium" id={groupId}>
          What does this distribution serve?
        </span>
        <span className="text-xs text-muted-foreground">One source only: a repository, one of its versions, or a publication.</span>
      </div>
      <div aria-labelledby={groupId} className="grid grid-cols-1 gap-2 sm:grid-cols-2" role="radiogroup">
        {options.map((o) => (
          <button
            key={o}
            aria-checked={mode === o}
            className={cn(
              "flex cursor-pointer flex-col gap-0.5 rounded-lg border bg-background p-3 text-left transition-colors outline-none hover:border-primary/50 focus-visible:ring-2 focus-visible:ring-ring",
              mode === o && "border-primary bg-primary/5 ring-1 ring-primary",
            )}
            role="radio"
            type="button"
            onClick={() => choose(o)}
          >
            <span className="flex items-center gap-2 text-sm font-medium">
              <span className={cn("size-3.5 shrink-0 rounded-full border", mode === o ? "border-4 border-primary" : "border-muted-foreground/50")} />
              {SOURCE_TEXT[o].title}
            </span>
            <span className="ps-5.5 text-xs text-muted-foreground">{SOURCE_TEXT[o].text}</span>
          </button>
        ))}
      </div>
      {(mode === "repository_version" || mode === "publication") && repoType?.list_path && (
        <Field>
          <FieldLabel>{mode === "repository_version" ? "Repository" : "Repository (filters the publications)"}</FieldLabel>
          <HrefPicker
            type={repoType}
            value={helperRepo}
            onChange={(v) => setState((s) => ({...s, values: {...s.values, [SOURCE_REPO]: v, ...(mode === "repository_version" ? {repository_version: ""} : {})}}))}
          />
        </Field>
      )}
      {field && !(mode === "repository_version" && !helperRepo) && <FieldRow errors={errors} field={field} setState={setState} state={state} />}
    </div>
  );
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
    <Field data-invalid={!!err || undefined}>
      <FieldLabel id={labelId}>
        {fieldLabel(field.name)}
        {field.required && <span className="text-destructive">*</span>}
      </FieldLabel>
      <FieldInput
        field={field}
        file={state.files[field.name] ?? null}
        invalid={!!err}
        labelId={labelId}
        value={state.values[field.name] ?? ""}
        onChange={(v) => setState((s) => ({...s, values: {...s.values, [field.name]: v}}))}
        onFile={(f) => setState((s) => ({...s, files: {...s.files, [field.name]: f}}))}
      />
      {err ? <FieldError>{err.join(" ")}</FieldError> : field.help ? <FieldDescription>{field.help}</FieldDescription> : null}
    </Field>
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
    <Dialog open={!!spec} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="flex max-h-[90vh] flex-col sm:max-w-2xl">
        {spec && <FormBody key={spec.path + spec.title} spec={spec} onClose={onClose} />}
      </DialogContent>
    </Dialog>
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
  const [showAdvanced, setShowAdvanced] = useState(false);

  const contextType = registry.typeForHref(spec.editHref ?? spec.path);
  const formRepository =
    contextType?.category === "repositories" && !contextType.parent
      ? spec.editHref ?? (spec.path !== contextType.list_path ? spec.path.replace(/[^/]+\/$/, "") : undefined)
      : undefined;
  const formContext: FormContextValue = {values: state.values, type: contextType, repository: formRepository};

  // Distributions: repository, version and publication are one explicit choice.
  const sourceFields =
    contextType?.category === "distributions" && contextType.name !== "pull-through" ? fields.filter((f) => (SOURCES as readonly string[]).includes(f.name) && f.kind.k === "relation") : [];
  const chooser = sourceFields.length >= 2;
  const inChooser = (f: FieldDef) => chooser && sourceFields.includes(f);
  const primary = fields.filter((f) => !inChooser(f) && (f.required || PRIMARY.has(f.name) || (spec.initial && f.name in spec.initial)));
  const advanced = fields.filter((f) => !inChooser(f) && !primary.includes(f));

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
      <DialogHeader>
        <DialogTitle>{spec.title}</DialogTitle>
      </DialogHeader>
      <div className="-mx-4 min-h-0 overflow-y-auto px-4">
        <FormContext.Provider value={formContext}>
        <form
          className="flex flex-col gap-5 py-1"
          id="schema-form"
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          {spec.intro}
          {spec.editHref && loaded.isLoading && <Spinner className="text-muted-foreground" />}
          {!fields.length && <p className="text-sm text-muted-foreground">This action takes no parameters.</p>}
          {primary.map((f) => (
            <FieldRow key={f.name} errors={errors} field={f} setState={setState} state={state} />
          ))}
          {chooser && <SourceChooser errors={errors} fields={sourceFields} plugin={spec.plugin} setState={setState} state={state} />}
          {advanced.length > 0 && (
            <Collapsible open={showAdvanced || advanced.some((f) => errors[f.name])} onOpenChange={setShowAdvanced}>
              <CollapsibleTrigger asChild>
                <Button className="group w-full justify-between" type="button" variant="ghost">
                  Advanced options ({advanced.length})
                  <ChevronDown className="transition-transform group-data-[state=open]:rotate-180" />
                </Button>
              </CollapsibleTrigger>
              <CollapsibleContent className="flex flex-col gap-5 pt-3">
                {advanced.map((f) => (
                  <FieldRow key={f.name} errors={errors} field={f} setState={setState} state={state} />
                ))}
              </CollapsibleContent>
            </Collapsible>
          )}
          {general && <p className="text-sm text-destructive">{general}</p>}
          {confirming && (
            <p className="text-warning text-sm font-medium">This operation is destructive. Press the button again to confirm.</p>
          )}
        </form>
        </FormContext.Provider>
      </div>
      <DialogFooter>
        <DialogClose asChild>
          <Button variant="outline">Cancel</Button>
        </DialogClose>
        <Button disabled={pending} form="schema-form" type="submit" variant={spec.danger ? "destructive" : "default"}>
          {pending && <Spinner />}
          {confirming ? "Confirm" : (spec.submitLabel ?? (spec.method === "PATCH" ? "Save" : "Submit"))}
        </Button>
      </DialogFooter>
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
