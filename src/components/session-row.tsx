import { memo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Folder, MessageSquare, Trash2 } from "lucide-react";
import { toast } from "sonner";

import type { SessionMeta } from "@/types";
import { formatClock, shortPath } from "@/lib/format";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

interface SessionRowProps {
  session: SessionMeta;
  onDeleted: (filePath: string) => void;
  onResume: (session: SessionMeta) => void;
}

function SessionRowBase({ session, onDeleted, onResume }: SessionRowProps) {
  const [open, setOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);

  async function handleDelete() {
    setDeleting(true);
    try {
      await invoke("delete_session", { path: session.filePath });
      onDeleted(session.filePath);
      setOpen(false);
      toast.success("会话已移入回收站");
    } catch (error) {
      toast.error(`删除失败:${String(error)}`);
    } finally {
      setDeleting(false);
    }
  }

  return (
    <div
      className="group relative select-none overflow-hidden rounded-xl border border-border/70 bg-card px-3 py-2.5 shadow-xs transition-colors hover:border-border"
      onDoubleClick={() => onResume(session)}
    >
      <div className="min-w-0 space-y-1">
        <Tooltip>
          <TooltipTrigger asChild>
            <div className="truncate text-[13px] font-medium leading-5">{session.title}</div>
          </TooltipTrigger>
          <TooltipContent side="top" align="start" className="max-w-64">
            <p className="break-words">{session.title}</p>
          </TooltipContent>
        </Tooltip>

        <div className="flex items-center gap-2">
          <span className="inline-flex min-w-0 max-w-full items-center gap-1 rounded-md bg-muted px-1.5 py-0.5">
            <Folder className="size-3.5 shrink-0" />
            <span className="truncate font-mono text-[11px] leading-none">
              {shortPath(session.cwd)}
            </span>
          </span>
          <span className="ml-auto inline-flex shrink-0 items-center gap-1 rounded-md bg-muted/60 px-1.5 py-0.5 text-[11px] leading-none text-muted-foreground tabular-nums">
            <MessageSquare className="size-3.5 shrink-0" />
            {session.userMessageCount} · {formatClock(session.lastActive)}
          </span>
        </div>
      </div>

      {/* 悬浮在右侧的删除按钮:不占卡片宽度,悬停时连同渐变遮罩一起淡入 */}
      <div
        className="pointer-events-none absolute inset-y-0 right-0 flex w-20 items-center justify-end bg-linear-to-r from-transparent via-card via-50% to-card pr-1.5 opacity-0 transition-opacity group-hover:pointer-events-auto group-hover:opacity-100 focus-within:pointer-events-auto focus-within:opacity-100"
        onDoubleClick={(event) => event.stopPropagation()}
      >
        <AlertDialog open={open} onOpenChange={setOpen}>
          <AlertDialogTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              title="删除会话"
              className="text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
            >
              <Trash2 className="size-4" />
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>删除这个会话?</AlertDialogTitle>
              <AlertDialogDescription>
                会话文件将移入系统回收站,之后可以恢复。正在进行的会话建议先退出再删除。
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={deleting}>取消</AlertDialogCancel>
              <AlertDialogAction
                disabled={deleting}
                className="bg-destructive text-white hover:bg-destructive/90"
                onClick={(event) => {
                  event.preventDefault();
                  void handleDelete();
                }}
              >
                {deleting ? "删除中…" : "移入回收站"}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </div>
  );
}

/** 用 memo 包裹:搜索等状态更新时,内容未变化的行跳过重渲染 */
export const SessionRow = memo(SessionRowBase);
