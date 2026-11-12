import {CircleAlert, Info} from "lucide-react";
import {type ReactNode, useState} from "react";

import {errorText} from "../lib/session";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogMedia,
  AlertDialogTitle,
} from "./ui/alert-dialog";
import {Button} from "./ui/button";
import {Dialog, DialogClose, DialogContent, DialogFooter, DialogHeader, DialogTitle} from "./ui/dialog";
import {Spinner} from "./ui/spinner";
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
    <AlertDialog
      open={isOpen}
      onOpenChange={(open) => {
        if (pending) return;
        if (!open) setError(null);
        onOpenChange(open);
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogMedia className={danger ? "bg-destructive/10 text-destructive" : "bg-primary/10 text-primary"}>
            {danger ? <CircleAlert /> : <Info />}
          </AlertDialogMedia>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription asChild>
            <div>
              {children}
              {error && <p className="mt-2 text-sm text-destructive">{error}</p>}
            </div>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={pending}>Cancel</AlertDialogCancel>
          {/* A plain button: AlertDialogAction would close the dialog before the action ends. */}
          <Button disabled={pending} variant={danger ? "destructive" : "default"} onClick={() => void run()}>
            {pending && <Spinner />}
            {confirmLabel ?? "Confirm"}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
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
    <Dialog open={value !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <div className="min-w-0">
          <JsonView value={value} />
        </div>
        <DialogFooter>
          <DialogClose asChild>
            <Button variant="outline">Close</Button>
          </DialogClose>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
