import { useState, useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Loader2, ChevronDown, ChevronRight } from 'lucide-react';
import { listLogs, listTasks, type ExecutionLog } from '@/lib/api';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';

interface HistoryViewProps {
  onBack: () => void;
}

function formatRelativeTime(isoString: string): string {
  const date = new Date(isoString);
  if (isNaN(date.getTime())) return 'unknown';
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffSec = Math.floor(diffMs / 1000);

  if (diffSec < 0) return 'just now';
  if (diffSec < 60) return `${diffSec}s ago`;
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  const diffDay = Math.floor(diffHr / 24);
  if (diffDay < 7) return `${diffDay}d ago`;

  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
}

function StatusBadge({ status }: { status: ExecutionLog['status'] }) {
  if (status === 'success') {
    return (
      <Badge className="text-[10px] h-4 px-1.5 bg-green-500/15 text-green-700 dark:text-green-400 border-transparent">
        success
      </Badge>
    );
  }
  if (status === 'failure') {
    return (
      <Badge variant="destructive" className="text-[10px] h-4 px-1.5">
        failure
      </Badge>
    );
  }
  return (
    <Badge variant="secondary" className="text-[10px] h-4 px-1.5 text-muted-foreground">
      skipped
    </Badge>
  );
}

function LogEntry({ log, taskName }: { log: ExecutionLog; taskName: string }) {
  const [expanded, setExpanded] = useState(false);
  const hasDetails = log.stdout || log.stderr || log.error;

  return (
    <div className="flex flex-col px-3 py-2 hover:bg-muted/50 border-b border-border/40 last:border-b-0">
      <div className="flex items-center gap-2">
        {/* Expand toggle */}
        <button
          onClick={() => setExpanded((v) => !v)}
          disabled={!hasDetails}
          className="shrink-0 text-muted-foreground disabled:opacity-30"
          aria-label={expanded ? 'Collapse details' : 'Expand details'}
        >
          {expanded ? (
            <ChevronDown className="size-3" />
          ) : (
            <ChevronRight className="size-3" />
          )}
        </button>

        {/* Task name */}
        <span className="text-xs font-medium truncate flex-1 min-w-0" title={taskName}>
          {taskName}
        </span>

        {/* Status */}
        <StatusBadge status={log.status} />

        {/* Timestamp */}
        <span className="text-[10px] text-muted-foreground shrink-0 ml-1">
          {formatRelativeTime(log.started_at)}
        </span>
      </div>

      {/* Expandable details */}
      {expanded && hasDetails && (
        <div className="mt-2 ml-5 flex flex-col gap-1.5">
          {log.error && (
            <div>
              <p className="text-[10px] font-semibold text-destructive mb-0.5">Error</p>
              <pre className="text-[10px] text-destructive bg-destructive/5 rounded px-2 py-1 whitespace-pre-wrap break-all">
                {log.error}
              </pre>
            </div>
          )}
          {log.stdout && (
            <div>
              <p className="text-[10px] font-semibold text-muted-foreground mb-0.5">stdout</p>
              <pre className="text-[10px] text-foreground bg-muted rounded px-2 py-1 whitespace-pre-wrap break-all max-h-32 overflow-y-auto">
                {log.stdout}
              </pre>
            </div>
          )}
          {log.stderr && (
            <div>
              <p className="text-[10px] font-semibold text-muted-foreground mb-0.5">stderr</p>
              <pre className="text-[10px] text-foreground bg-muted rounded px-2 py-1 whitespace-pre-wrap break-all max-h-32 overflow-y-auto">
                {log.stderr}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default function HistoryView({ onBack }: HistoryViewProps) {
  const {
    data: logs,
    isLoading: logsLoading,
    error: logsError,
    refetch,
  } = useQuery({
    queryKey: ['logs'],
    queryFn: () => listLogs({ limit: 50 }),
    staleTime: 10_000,
  });

  const { data: tasks } = useQuery({
    queryKey: ['tasks'],
    queryFn: listTasks,
    staleTime: 30_000,
  });

  // Build a quick id→name lookup map
  const taskNameById = useMemo(
    () => new Map<string, string>(tasks?.map((t) => [t.id, t.name]) ?? []),
    [tasks]
  );

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 h-9 border-b shrink-0">
        <button
          onClick={onBack}
          className="text-xs text-muted-foreground hover:text-foreground leading-none"
          aria-label="Back to task list"
        >
          ← Back
        </button>
        <span className="text-xs font-semibold flex-1">Execution History</span>
      </div>

      {/* Body */}
      <div className="flex-1 overflow-y-auto">
        {logsLoading && (
          <div className="flex items-center justify-center h-full text-muted-foreground">
            <Loader2 className="size-4 animate-spin mr-2" />
            <span className="text-xs">Loading history…</span>
          </div>
        )}

        {logsError && (
          <div className="flex flex-col items-center justify-center h-full gap-2 p-4 text-center">
            <p className="text-xs text-destructive">
              {logsError instanceof Error ? logsError.message : 'Failed to load history'}
            </p>
            <Button variant="outline" size="xs" onClick={() => refetch()}>
              Retry
            </Button>
          </div>
        )}

        {!logsLoading && !logsError && (!logs || logs.length === 0) && (
          <div className="flex items-center justify-center h-full">
            <p className="text-xs text-muted-foreground">No execution history yet.</p>
          </div>
        )}

        {!logsLoading && !logsError && logs && logs.length > 0 && (
          <div className="py-1">
            {logs.map((log) => (
              <LogEntry
                key={log.id}
                log={log}
                taskName={taskNameById.get(log.task_id) ?? log.task_id}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
