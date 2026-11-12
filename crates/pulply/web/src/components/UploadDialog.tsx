// Chunked uploads from the browser (`uploads/` API through the proxy), each file then
// turned into content, optionally added to a repository.

import {CloudArrowUpIn} from "@gravity-ui/icons";
import {Button, Label, ListBox, Modal, ProgressBar, Select} from "@heroui/react";
import {useQueryClient} from "@tanstack/react-query";
import {useState} from "react";

import {type Json, json, request} from "../lib/api";
import {bytes, typeTitle} from "../lib/format";
import type {ResourceType} from "../lib/registry";
import {errorText, useSession} from "../lib/session";
import {HrefPicker} from "./HrefPicker";

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
    <Modal.Backdrop isOpen isDismissable={!running} onOpenChange={(open) => !open && !running && onClose()}>
      <Modal.Container scroll="inside" size="lg">
        <Modal.Dialog>
          <Modal.CloseTrigger />
          <Modal.Header>
            <Modal.Heading>Upload content</Modal.Heading>
          </Modal.Header>
          <Modal.Body className="flex flex-col gap-4">
            {types.length > 1 && (
              <Select fullWidth isDisabled={running} value={typeId} onChange={(k) => setTypeId(String(k))}>
                <Label>Content type</Label>
                <Select.Trigger>
                  <Select.Value />
                  <Select.Indicator />
                </Select.Trigger>
                <Select.Popover>
                  <ListBox>
                    {types.map((t) => (
                      <ListBox.Item key={t.id} id={t.id} textValue={typeTitle(t)}>
                        {typeTitle(t)}
                        <ListBox.ItemIndicator />
                      </ListBox.Item>
                    ))}
                  </ListBox>
                </Select.Popover>
              </Select>
            )}
            {acceptsRepo && repoType && !fixedRepo && (
              <div className="flex flex-col gap-1.5">
                <HrefPicker label="Add to repository (optional)" type={repoType} value={repository} onChange={setRepository} />
              </div>
            )}
            <label
              className={`flex flex-col items-center justify-center gap-2 rounded-2xl border-2 border-dashed p-8 text-center cursor-pointer transition-colors ${
                dragging ? "border-accent bg-accent-soft" : "border-default hover:bg-default/40"
              }`}
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
              <CloudArrowUpIn className="size-8 text-accent" />
              <span className="font-medium">Drop files here or click to browse</span>
              <span className="text-xs text-muted">Sent in chunks of {bytes(chunkSize)}</span>
              <input multiple className="hidden" disabled={running} type="file" onChange={(e) => add(e.target.files)} />
            </label>
            {items.length > 0 && (
              <ul className="flex flex-col gap-3">
                {items.map((it, i) => (
                  <li key={i} className="flex flex-col gap-1">
                    <div className="flex items-center justify-between gap-2 text-sm">
                      <span className="truncate font-medium">{it.file.name}</span>
                      <span className={it.phase === "failed" ? "text-danger" : "text-muted"}>
                        {it.phase === "uploading" ? `${bytes(it.sent)} / ${bytes(it.file.size)}` : it.phase}
                      </span>
                    </div>
                    <ProgressBar
                      aria-label={`Upload of ${it.file.name}`}
                      color={it.phase === "failed" ? "danger" : it.phase === "done" ? "success" : "accent"}
                      isIndeterminate={it.phase === "creating"}
                      size="sm"
                      value={it.file.size ? (it.sent / it.file.size) * 100 : it.phase === "done" ? 100 : 0}
                    >
                      <ProgressBar.Track>
                        <ProgressBar.Fill />
                      </ProgressBar.Track>
                    </ProgressBar>
                    {it.error && <span className="text-xs text-danger">{it.error}</span>}
                  </li>
                ))}
              </ul>
            )}
          </Modal.Body>
          <Modal.Footer>
            <Button isDisabled={running} slot="close" variant="tertiary">
              {finished ? "Close" : "Cancel"}
            </Button>
            {!finished && (
              <Button isDisabled={!items.length || !type || running} isPending={running} onPress={() => void start()}>
                Upload {items.length || ""} file{items.length > 1 ? "s" : ""}
              </Button>
            )}
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}
