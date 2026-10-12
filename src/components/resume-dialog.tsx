import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TerminalSquare } from "lucide-react";
import { toast } from "sonner";

import type { SessionMeta, TerminalInfo } from "@/types";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";

interface ResumeDialogProps {
  session: SessionMeta | null;
  terminals: TerminalInfo[];
  onOpenChange: (open: boolean) => void;
}

export function ResumeDialog({ session, terminals, onOpenChange }: ResumeDialogProps) {
  const [busy, setBusy] = useState(false);

  async function pick(terminalId: string) {
    if (!session || busy) return;
    setBusy(true);
    try {
      await invoke("resume_session", {
        sessionId: session.sessionId,
        cwd: session.cwd,
        terminal: terminalId,
      });
      onOpenChange(false);
    } catch (error) {
      toast.error(`恢复会话失败:${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={session !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[320px]">
        <DialogHeader>
          <DialogTitle>用哪个终端恢复会话?</DialogTitle>
        </DialogHeader>
        {terminals.length === 0 ? (
          <p className="text-xs text-muted-foreground">未检测到可用的终端</p>
        ) : (
          <div className="flex flex-col gap-1.5">
            {terminals.map((terminal) => (
              <Button
                key={terminal.id}
                variant="outline"
                disabled={busy}
                className="justify-start font-normal"
                onClick={() => void pick(terminal.id)}
              >
                <TerminalSquare className="size-4 shrink-0 text-muted-foreground" />
                <span className="min-w-0 truncate">{terminal.name}</span>
              </Button>
            ))}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
