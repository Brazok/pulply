// Chunked uploads from the browser (`uploads/` API through the proxy), each file then
// turned into content, optionally added to a repository.

import {useQueryClient} from "@tanstack/react-query";
import {CloudUpload} from "lucide-react";
import {useState} from "react";

import {type Json, json, request} from "../lib/api";
import {bytes, typeTitle} from "../lib/format";
import type {ResourceType} from "../lib/registry";
import {errorText, useSession} from "../lib/session";
import {cn} from "../lib/utils";
import {HrefPicker} from "./HrefPicker";
import {Button} from "./ui/button";
import {Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle} from "./ui/dialog";
import {Field, FieldLabel} from "./ui/field";
import {Progress} from "./ui/progress";
import {Select, SelectContent, SelectItem, SelectTrigger, SelectValue} from "./ui/select";
import {Spinner} from "./ui/spinner";

interface Item {
  file: File;
  sent: number;
  phase: "queued" | "uploading" | "creating" | "done" | "failed";
  error?: string;
}

export function UploadDialog({
  type: initialType,
  repository: fixedRepo,
  onClose,
}: {
  type?: ResourceType;
  /** Repository the content is added to (upload from a repository page). */
  repository?: string;
  onClose: () => void;
}) {
  const {registry, pulp, info, chunkSize, track, ctx} = useSession();
  const queryClient = useQueryClient();
  const repoPlugin = fixedRepo ? registry.typeForHref(fixedRepo)?.plugin : undefined;
  const types = registry.uploadTypes(initialType?.plugin ?? repoPlugin);
  const [typeId, setTypeId] = useState<string>(initialType?.id ?? types.find((t) => /package|file|artifact/.test(t.name))?.id ?? types[0]?.id ?? "");
  const type = registry.get(typeId);
  const schema = registry.schema(type?.create_schema);
  const repoType = type ? registry.pluginType("repositories", type.plugin, false) : undefined;
  const acceptsRepo = !!schema?.properties?.repository;
  const [repository, setRepository] = useState(fixedRepo ?? "");
  const [items, setItems] = useState<Item[]>([]);
  const [running, setRunning] = useState(false);
  const [dragging, setDragging] = useState(false);

  const add = (files: FileList | null) => {
    if (!files) return;
    setItems((cur) => [...cur, ...[...files].map((file) => ({file, sent: 0, phase: "queued" as const}))]);
  };
  const update = (i: number, patch: Partial<Item>) => setItems((cur) => cur.map((it, j) => (j === i ? {...it, ...patch} : it)));

  const uploadOne = async (item: Item, i: number) => {
    const {file} = item;
    update(i, {phase: "uploading", sent: 0});
    const created = await pulp.post(`${info.api_prefix}uploads/`, {size: file.size});
    const href = created.pulp_href as string;
    for (let start = 0; start < file.size || (start === 0 && file.size === 0); start += chunkSize) {
      const end = Math.min(start + chunkSize, file.size);
      const form = new FormData();
      form.append("file", file.slice(start, end), file.name);
      await request(pulp.url(href), {
        method: "PUT",
        headers: {"Content-Range": `bytes ${start}-${Math.max(end - 1, 0)}/${file.size}`},
        body: form,
      });
      update(i, {sent: end});
      if (file.size === 0) break;
    }
    update(i, {phase: "creating"});
    const fields: Record<string, string> = {upload: href};
    if (schema?.properties?.relative_path) fields.relative_path = file.name;
    if (acceptsRepo && repository) fields.repository = repository;
    const resp = await json<Json>(pulp.url(type!.list_path!), {
      method: "POST",
      headers: {"Content-Type": "application/x-www-form-urlencoded"},
      body: new URLSearchParams(fields),
    });
    track(resp, `Upload ${file.name}`);
    update(i, {phase: "done"});
  };

  const start = async () => {
    setRunning(true);
    for (let i = 0; i < items.length; i++) {
      if (items[i].phase === "done") continue;
      try {
        await uploadOne(items[i], i);
      } catch (e) {
        update(i, {phase: "failed", error: errorText(e)});
      }
    }
    setRunning(false);
    queryClient.invalidateQueries({queryKey: ["pulp", ctx]});
  };

  const finished = items.length > 0 && items.every((i) => i.phase === "done" || i.phase === "failed");

  return (
    <Dialog open onOpenChange={(open) => !open && !running && onClose()}>
      <DialogContent className="flex max-h-[90vh] flex-col sm:max-w-2xl" showCloseButton={!running}>
        <DialogHeader>
          <DialogTitle>Upload content</DialogTitle>
        </DialogHeader>
        <div className="-mx-4 flex min-h-0 flex-col gap-4 overflow-y-auto px-4">
            {types.length > 1 && (
              <Field>
                <FieldLabel htmlFor="upload-type">Content type</FieldLabel>
                <Select disabled={running} value={typeId} onValueChange={setTypeId}>
                  <SelectTrigger className="w-full" id="upload-type">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {types.map((t) => (
                      <SelectItem key={t.id} value={t.id}>
                        {typeTitle(t)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
            )}
            {acceptsRepo && repoType && !fixedRepo && (
              <HrefPicker label="Add to repository (optional)" type={repoType} value={repository} onChange={setRepository} />
            )}
            <label
              className={cn(
                "flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed p-8 text-center transition-colors",
                dragging ? "border-primary bg-primary/5" : "hover:bg-muted/50",
              )}
              onDragLeave={() => setDragging(false)}
              onDragOver={(e) => {
                e.preventDefault();
                setDragging(true);
              }}
              onDrop={(e) => {
                e.preventDefault();
                setDragging(false);
                add(e.dataTransfer.files);
              }}
            >
              <CloudUpload className="size-8 text-primary" />
              <span className="font-medium">Drop files here or click to browse</span>
              <span className="text-xs text-muted-foreground">Sent in chunks of {bytes(chunkSize)}</span>
              <input multiple className="hidden" disabled={running} type="file" onChange={(e) => add(e.target.files)} />
            </label>
            {items.length > 0 && (
              <ul className="flex flex-col gap-3">
                {items.map((it, i) => (
                  <li key={i} className="flex flex-col gap-1">
                    <div className="flex items-center justify-between gap-2 text-sm">
                      <span className="truncate font-medium">{it.file.name}</span>
                      <span className={it.phase === "failed" ? "text-destructive" : "text-muted-foreground"}>
                        {it.phase === "uploading" ? `${bytes(it.sent)} / ${bytes(it.file.size)}` : it.phase}
                      </span>
                    </div>
                    <Progress
                      aria-label={`Upload of ${it.file.name}`}
                      className={cn(
                        it.phase === "failed" && "*:data-[slot=progress-indicator]:bg-destructive",
                        it.phase === "done" && "*:data-[slot=progress-indicator]:bg-success",
                      )}
                      value={it.phase === "creating" ? null : it.file.size ? (it.sent / it.file.size) * 100 : it.phase === "done" ? 100 : 0}
                    />
                    {it.error && <span className="text-xs text-destructive">{it.error}</span>}
                  </li>
                ))}
              </ul>
            )}
        </div>
        <DialogFooter>
          <Button disabled={running} variant="outline" onClick={onClose}>
            {finished ? "Close" : "Cancel"}
          </Button>
          {!finished && (
            <Button disabled={!items.length || !type || running} onClick={() => void start()}>
              {running && <Spinner />}
              Upload {items.length || ""} file{items.length > 1 ? "s" : ""}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
