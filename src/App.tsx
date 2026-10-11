import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { BookOpen, Library, Minus, RefreshCw, Search, X } from "lucide-react";
import { toast } from "sonner";

import type { SessionMeta, TerminalInfo } from "@/types";
import { dayLabel } from "@/lib/format";
import { cn } from "cn";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SessionRow } from "@/components/session-row";
import { ResumeDialog } from "@/components/resume-dialog";

const appWindow = getCurrentWindow();

export default function App() {
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [terminals, setTerminals] = useState<TerminalInfo[]>([]);
  const [resumeTarget, setResumeTarget] = useState<SessionMeta | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const list = await invoke<SessionMeta[]>("list_sessions");
      setSessions(list);
    } catch (error) {
      toast.error(`读取会话失败:${String(error)}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    invoke<TerminalInfo[]>("list_terminals")
      .then(setTerminals)
      .catch(() => {
        // 检测失败时保持空列表,打开对话框时会展示为空,不影响其他功能
      });
  }, []);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return sessions;
    return sessions.filter(
      (s) => s.title.toLowerCase().includes(q) || s.cwd.toLowerCase().includes(q),
    );
  }, [sessions, query]);

  const groups = useMemo(() => {
    const map = new Map<string, SessionMeta[]>();
    for (const session of filtered) {
      const label = dayLabel(session.lastActive);
      const bucket = map.get(label);
      if (bucket) bucket.push(session);
      else map.set(label, [session]);
    }
    return Array.from(map.entries());
  }, [filtered]);

  const handleDeleted = useCallback((filePath: string) => {
    setSessions((prev) => prev.filter((s) => s.filePath !== filePath));
  }, []);

  const showSkeleton = loading && sessions.length === 0;

  return (
    <TooltipProvider delayDuration={200}>
      <div className="relative flex h-full flex-col overflow-hidden rounded-lg bg-background text-foreground">
        {/* 窗口描边:改为内嵌一层浮层绘制,并让底部抬高 1px,避开透明窗口在 WebView2 下的边缘裁切 */}
        <div className="pointer-events-none absolute top-0 right-0 bottom-px left-0 z-50 rounded-lg border-2 border-border" />
        {/* 自绘顶栏(窗口无边框):整条可拖动,右侧为最小化 / 关闭 */}
        <div
          className="flex h-9 shrink-0 select-none items-center justify-end gap-0.5 px-1.5"
          data-tauri-drag-region
        >
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => void appWindow.minimize()}
            title="最小化"
            className="text-muted-foreground"
          >
            <Minus className="size-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => void appWindow.close()}
            title="关闭"
            className="text-muted-foreground hover:bg-destructive hover:text-white"
          >
            <X className="size-4" />
          </Button>
        </div>

        <header className="space-y-3 border-b border-border/60 px-4 pt-3 pb-3.5">
          <div className="flex items-center gap-2.5" data-tauri-drag-region>
            <div
              className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-zinc-900 text-zinc-50 shadow-sm"
              data-tauri-drag-region
            >
              <Library className="size-4" />
            </div>
            <div className="min-w-0" data-tauri-drag-region>
              <h1
                className="text-sm font-semibold leading-none tracking-tight"
                data-tauri-drag-region
              >
                Shelf
              </h1>
              <p className="mt-1 truncate text-[11px] text-muted-foreground" data-tauri-drag-region>
                Claude Code 会话 · 共 {sessions.length} 个
                {query.trim() !== "" && ` · 匹配 ${filtered.length} 个`}
              </p>
            </div>
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => void load()}
              disabled={loading}
              title="刷新"
              className="ml-auto shrink-0 text-muted-foreground"
            >
              <RefreshCw className={cn("size-4", loading && "animate-spin")} />
            </Button>
          </div>

          <div className="relative">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="搜索标题 / 目录…"
              className="h-8 pl-8 text-[13px]"
            />
          </div>
        </header>

        <main className="flex-1 overflow-y-auto">
          {showSkeleton ? (
            <div className="space-y-2 px-3 py-3">
              {Array.from({ length: 6 }).map((_, index) => (
                <Skeleton key={index} className="h-[64px] rounded-xl" />
              ))}
            </div>
          ) : groups.length === 0 ? (
            <div className="flex h-full flex-col items-center justify-center gap-3 px-4 text-center">
              <div className="flex size-14 items-center justify-center rounded-2xl bg-muted">
                <BookOpen className="size-7 text-muted-foreground" />
              </div>
              {query.trim() !== "" ? (
                <>
                  <p className="text-sm font-medium">没有匹配的会话</p>
                  <p className="text-xs text-muted-foreground">换个关键词试试</p>
                </>
              ) : (
                <>
                  <p className="text-sm font-medium">还没有找到任何会话</p>
                  <p className="text-xs text-muted-foreground">
                    会话记录存放在 <code className="font-mono">~/.claude/projects</code>
                  </p>
                </>
              )}
            </div>
          ) : (
            groups.map(([label, items]) => (
              <section key={label}>
                <div className="sticky top-0 z-10 border-b border-border/40 bg-background/85 px-3 py-1.5 text-xs font-medium text-muted-foreground backdrop-blur-sm">
                  {label}
                  <span className="ml-2 opacity-60">{items.length}</span>
                </div>
                <div className="space-y-1.5 px-3 py-2.5">
                  {items.map((session) => (
                    <SessionRow
                      key={session.filePath}
                      session={session}
                      onDeleted={handleDeleted}
                      onResume={setResumeTarget}
                    />
                  ))}
                </div>
              </section>
            ))
          )}
        </main>

        <ResumeDialog
          session={resumeTarget}
          terminals={terminals}
          onOpenChange={(open) => {
            if (!open) setResumeTarget(null);
          }}
        />
        <Toaster />
      </div>
    </TooltipProvider>
  );
}
