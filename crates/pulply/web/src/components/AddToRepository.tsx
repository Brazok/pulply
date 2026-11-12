import {Button, Modal} from "@heroui/react";
import {useState} from "react";

import type {ResourceType} from "../lib/registry";
import {errorText, useSession} from "../lib/session";
import {HrefPicker} from "./HrefPicker";

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
    <Modal.Backdrop isOpen onOpenChange={(open) => !open && onClose()}>
      <Modal.Container>
        <Modal.Dialog>
          <Modal.CloseTrigger />
          <Modal.Header>
            <Modal.Heading>
              Add {units.length} unit{units.length > 1 ? "s" : ""} to a repository
            </Modal.Heading>
          </Modal.Header>
          <Modal.Body className="flex flex-col gap-3">
            <HrefPicker label="Repository" type={repoType} value={repo} onChange={setRepo} />
            <p className="text-xs text-muted">A new repository version is created with these units added.</p>
            {error && <p className="text-sm text-danger">{error}</p>}
          </Modal.Body>
          <Modal.Footer>
            <Button slot="close" variant="tertiary">
              Cancel
            </Button>
            <Button isDisabled={!repo} isPending={pending} onPress={() => void submit()}>
              Add
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}
