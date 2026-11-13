// Copy to the clipboard. `navigator.clipboard` only exists on secure origins (HTTPS or
// localhost): the interface served on the network over HTTP falls back to a hidden textarea.

import {Copy} from "lucide-react";
import {toast} from "sonner";

import {Button} from "./ui/button";
import {Tooltip, TooltipContent, TooltipTrigger} from "./ui/tooltip";

export async function copyText(text: string, done = "Copied to the clipboard"): Promise<void> {
  try {
    if (!navigator.clipboard) throw new Error("no clipboard API");
    await navigator.clipboard.writeText(text);
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand("copy");
    area.remove();
    if (!ok) {
      toast.error("Cannot copy: select the text and copy it by hand");
      return;
    }
  }
  toast.success(done, {duration: 1500});
}

export function CopyButton({text, label = "Copy"}: {text: string; label?: string}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button aria-label={label} size="icon-xs" variant="ghost" onClick={() => void copyText(text)}>
          <Copy />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
