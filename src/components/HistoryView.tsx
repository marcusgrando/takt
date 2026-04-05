import { useState, useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Loader2, ChevronDown, ChevronRight } from 'lucide-react';
import { listLogs, listTasks, type ExecutionLog } from '@/lib/api';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';

interface HistoryViewProps {
  onBack: () => void;
}

function formatRelativeTime(isoString: string): string {
  const date = new Date(isoString);
  if (isNaN(date.getTime())) return 'unknown';
  const diffSec = Math.floor((Date.now() - date.getTime()) / 1000);
  if (diffSec < 0) return 'just now';
  if (diffSec < 60) return `${diffSec}s ago`;
  const m = Math.floor(diffSec / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d}d ago`;
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
}

function StatusBadge({ status }: { status: ExecutionLog['status'] }) {
  if (status === 'success') return <Badge className="bg-green-500/12 text-green-700 dark:text-green-400 border-transparent">success</Badge>;
  if (status === 'failure') return <Badge variant="destructive">failure</Badge>;
  if (status === 'schedule_error') return <Badge variant="destructive">schedule error</Badge>;
  return <Badge variant="secondary">skipped</Badge>;
}

function LogEntry({ log, taskName }: { log: ExecutionLog; taskName: string }) {
  const [expanded, setExpanded] = useState(false);
  const hasDetails = log.stdout || log.stderr || log.error;

  return (
    <div className="flex flex-col gap-2 px-4 py-3 hover:bg-muted/50 transition-colors">
      <div className="flex items-center gap-2">
        <button
          onClick={() => setExpanded((v) => !v)}
          disabled={!hasDetails}
          className="shrink-0 text-muted-foreground hover:text-foreground disabled:opacity-20"
        >
          {expanded ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
        </button>
        <span className="text-sm font-medium truncate flex-1">{taskName}</span>
        <StatusBadge status={log.status} />
        <span className="text-xs text-muted-foreground shrink-0">{formatRelativeTime(log.started_at)}</span>
      </div>

      {expanded && hasDetails && (
        <div className="ml-6 space-y-2">
          {log.error && (
            <div className="space-y-1">
              <p className="text-xs font-semibold text-destructive">Error</p>
              <pre className="text-xs text-destructive bg-destructive/5 rounded-md px-3 py-2 whitespace-pre-wrap break-all font-mono">{log.error}</pre>
            </div>
          )}
          {log.stdout && (
            <div className="space-y-1">
              <p className="text-xs font-semibold text-muted-foreground">stdout</p>
              <pre className="text-xs bg-muted rounded-md px-3 py-2 whitespace-pre-wrap break-all max-h-32 overflow-y-auto font-mono">{log.stdout}</pre>
            </div>
          )}
          {log.stderr && (
            <div className="space-y-1">
              <p className="text-xs font-semibold text-muted-foreground">stderr</p>
              <pre className="text-xs bg-muted rounded-md px-3 py-2 whitespace-pre-wrap break-all max-h-32 overflow-y-auto font-mono">{log.stderr}</pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default function HistoryView({ onBack }: HistoryViewProps) {
  const { data: logs, isLoading, error, refetch } = useQuery({
    queryKey: ['logs'],
    queryFn: () => listLogs({ limit: 50 }),
    staleTime: 10_000,
  });
  const { data: tasks } = useQuery({ queryKey: ['tasks'], queryFn: listTasks, staleTime: 30_000 });
  const nameMap = useMemo(() => new Map(tasks?.map((t) => [t.id, t.name]) ?? []), [tasks]);

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center px-4 h-12 shrink-0">
        <Button variant="link" size="sm" onClick={onBack} className="px-0 text-primary">Back</Button>
        <span className="flex-1 text-center text-sm font-semibold">History</span>
        <span className="w-10" />
      </div>
      <Separator />

      <div className="flex-1 overflow-y-auto scrollbar-none">
        {isLoading && (
          <div className="flex items-center justify-center h-full gap-2 text-muted-foreground">
            <Loader2 className="size-4 animate-spin" /><span className="text-sm">Loading…</span>
          </div>
        )}
        {error && (
          <div className="flex flex-col items-center justify-center h-full gap-3 p-6">
            <p className="text-sm text-destructive">{error instanceof Error ? error.message : 'Failed'}</p>
            <Button variant="outline" size="sm" onClick={() => refetch()}>Retry</Button>
          </div>
        )}
        {!isLoading && !error && (!logs || logs.length === 0) && (
          <div className="flex flex-col items-center justify-center h-full gap-1 p-6">
            <p className="text-sm font-medium">No history yet</p>
            <p className="text-sm text-muted-foreground">Executions will appear here.</p>
          </div>
        )}
        {!isLoading && !error && logs && logs.length > 0 && (
          <div className="divide-y divide-border">
            {logs.map((log) => (
              <LogEntry key={log.id} log={log} taskName={nameMap.get(log.task_id) ?? log.task_id} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
