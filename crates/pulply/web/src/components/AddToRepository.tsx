import {useState} from "react";

import type {ResourceType} from "../lib/registry";
import {errorText, useSession} from "../lib/session";
import {HrefPicker} from "./HrefPicker";
import {Button} from "./ui/button";
import {Dialog, DialogClose, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle} from "./ui/dialog";
import {Spinner} from "./ui/spinner";

/** Adds content units to a repository (`modify/`), creating a new version. */
export function AddToRepository({
  repoType,
  units,
  onClose,
  onDone,
}: {
  repoType: ResourceType;
  units: string[];
  onClose: () => void;
  onDone?: () => void;
}) {
  const {pulp, track} = useSession();
  const [repo, setRepo] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const submit = async () => {
    setPending(true);
    setError(null);
    try {
      const resp = await pulp.post(`${repo}modify/`, {add_content_units: units});
      track(resp, `Add ${units.length} unit(s)`);
      onDone?.();
      onClose();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setPending(false);
    }
  };
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            Add {units.length} unit{units.length > 1 ? "s" : ""} to a repository
          </DialogTitle>
          <DialogDescription>A new repository version is created with these units added.</DialogDescription>
        </DialogHeader>
        <HrefPicker label="Repository" type={repoType} value={repo} onChange={setRepo} />
        {error && <p className="text-sm text-destructive">{error}</p>}
        <DialogFooter>
          <DialogClose asChild>
            <Button variant="outline">Cancel</Button>
          </DialogClose>
          <Button disabled={!repo || pending} onClick={() => void submit()}>
            {pending && <Spinner />}
            Add
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
