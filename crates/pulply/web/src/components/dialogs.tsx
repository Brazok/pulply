import {AlertDialog, Button, Modal, Spinner} from "@heroui/react";
import {type ReactNode, useState} from "react";

import {errorText} from "../lib/session";
import {JsonView} from "./values";

export interface ConfirmProps {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  children?: ReactNode;
  confirmLabel?: string;
  danger?: boolean;
  /** Runs on confirm; the dialog stays open (pending) until it resolves, and shows its error. */
  onConfirm: () => Promise<unknown> | void;
}

export function ConfirmDialog({isOpen, onOpenChange, title, children, confirmLabel, danger, onConfirm}: ConfirmProps) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = async () => {
    setPending(true);
    setError(null);
    try {
      await onConfirm();
      onOpenChange(false);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setPending(false);
    }
  };
  return (
    <AlertDialog.Backdrop
      isOpen={isOpen}
      onOpenChange={(open) => {
        if (!open) setError(null);
        onOpenChange(open);
      }}
    >
      <AlertDialog.Container>
        <AlertDialog.Dialog className="sm:max-w-[440px]">
          <AlertDialog.Header>
            <AlertDialog.Icon status={danger ? "danger" : "accent"} />
            <AlertDialog.Heading>{title}</AlertDialog.Heading>
          </AlertDialog.Header>
          <AlertDialog.Body>
            {children}
            {error && <p className="text-danger text-sm mt-2">{error}</p>}
          </AlertDialog.Body>
          <AlertDialog.Footer>
            <Button slot="close" variant="tertiary">
              Cancel
            </Button>
            <Button isPending={pending} variant={danger ? "danger" : "primary"} onPress={run}>
              {pending && <Spinner color="current" size="sm" />}
              {confirmLabel ?? "Confirm"}
            </Button>
          </AlertDialog.Footer>
        </AlertDialog.Dialog>
      </AlertDialog.Container>
    </AlertDialog.Backdrop>
  );
}

/** Read-only JSON result (synchronous operation responses, GET actions). */
export function ResultDialog({
  title,
  value,
  onClose,
}: {
  title: string;
  value: unknown | null;
  onClose: () => void;
}) {
  return (
    <Modal.Backdrop isOpen={value !== null} onOpenChange={(open) => !open && onClose()}>
      <Modal.Container size="lg">
        <Modal.Dialog>
          <Modal.CloseTrigger />
          <Modal.Header>
            <Modal.Heading>{title}</Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <JsonView value={value} />
          </Modal.Body>
          <Modal.Footer>
            <Button slot="close" variant="secondary">
              Close
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}
