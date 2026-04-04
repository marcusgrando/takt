import { useState } from 'react';
import { Pencil, Play, Trash2, Loader2 } from 'lucide-react';
import { Switch } from '@/components/ui/switch';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { queryClient } from '@/lib';
import { updateTask, runTaskNow, deleteTask, type TaskDto, type Action } from '@/lib/api';

interface TaskItemProps {
  task: TaskDto;
  onEdit: (id: string) => void;
  onDeleted: () => void;
}

function actionLabel(action: Action): string {
  const map: Record<Action['type'], string> = {
    RunCommand: 'shell', OpenUrl: 'url', Notify: 'notify',
    OpenFile: 'file', OpenApp: 'app', Webhook: 'webhook',
  };
  return map[action.type] ?? 'unknown';
}

function actionBadgeClass(action: Action): string {
  const map: Record<Action['type'], string> = {
    OpenUrl:    'bg-blue-500/15 text-blue-700 dark:text-blue-400 border-transparent',
    RunCommand: 'bg-purple-500/15 text-purple-700 dark:text-purple-400 border-transparent',
    Notify:     'bg-orange-500/15 text-orange-700 dark:text-orange-400 border-transparent',
    OpenFile:   'bg-green-500/15 text-green-700 dark:text-green-400 border-transparent',
    OpenApp:    'bg-cyan-500/15 text-cyan-700 dark:text-cyan-400 border-transparent',
    Webhook:    'bg-teal-500/15 text-teal-700 dark:text-teal-400 border-transparent',
  };
  return map[action.type] ?? 'border-transparent';
}

export default function TaskItem({ task, onEdit, onDeleted }: TaskItemProps) {
  const [isRunning, setIsRunning] = useState(false);
  const [isToggling, setIsToggling] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleToggle(checked: boolean) {
    if (isToggling) return;
    queryClient.setQueryData<TaskDto[]>(['tasks'], (prev) =>
      prev?.map((t) => t.id === task.id ? { ...t, enabled: checked } : t) ?? []
    );
    try {
      setIsToggling(true);
      await updateTask({ id: task.id, enabled: checked });
      await queryClient.invalidateQueries({ queryKey: ['tasks'] });
    } catch (err) {
      queryClient.setQueryData<TaskDto[]>(['tasks'], (prev) =>
        prev?.map((t) => t.id === task.id ? { ...t, enabled: task.enabled } : t) ?? []
      );
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsToggling(false);
    }
  }

  async function handleRun() {
    if (isRunning) return;
    setError(null);
    setIsRunning(true);
    try { await runTaskNow(task.id); }
    catch (err) { setError(err instanceof Error ? err.message : String(err)); }
    finally { setIsRunning(false); }
  }

  async function handleDelete() {
    setIsDeleting(true);
    setConfirmDelete(false);
    try { await deleteTask(task.id); onDeleted(); }
    catch (err) { setError(err instanceof Error ? err.message : String(err)); }
    finally { setIsDeleting(false); }
  }

  return (
    <div className="flex flex-col gap-1 px-4 py-2.5 hover:bg-muted/50 transition-colors">
      {/* Row 1: name only */}
      <div className="flex items-center min-w-0">
        <span className="text-sm font-medium truncate min-w-0">{task.name}</span>
      </div>

      {/* Row 2: status + badge + toggle + actions */}
      <div className="flex items-center gap-2">
        <div className={`size-1.5 rounded-full shrink-0 ${task.enabled ? 'bg-green-500' : 'bg-muted-foreground/25'}`} />
        <Badge variant="secondary" className={`${actionBadgeClass(task.action)} text-[10px] px-1.5 py-0 shrink-0`}>
          {actionLabel(task.action)}
        </Badge>
        <Switch
          checked={task.enabled}
          onCheckedChange={handleToggle}
          disabled={isToggling}
          aria-label={`Toggle ${task.name}`}
        />
        <div className="flex-1" />
        <div className="flex items-center -mr-1.5">
          <Button variant="ghost" size="icon-sm" onClick={handleRun} disabled={isRunning} title="Run now">
            {isRunning ? <Loader2 className="animate-spin" /> : <Play />}
          </Button>
          <Button variant="ghost" size="icon-sm" onClick={() => onEdit(task.id)} title="Edit">
            <Pencil />
          </Button>
          <Button
            variant="ghost" size="icon-sm"
            onClick={() => setConfirmDelete(true)}
            disabled={isDeleting} title="Delete"
            className="text-destructive hover:text-destructive hover:bg-destructive/10"
          >
            {isDeleting ? <Loader2 className="animate-spin" /> : <Trash2 />}
          </Button>
        </div>
      </div>

      {confirmDelete && (
        <div className="flex items-center justify-between rounded-md bg-destructive/10 px-3 py-2 text-sm">
          <span className="text-destructive font-medium">Delete?</span>
          <div className="flex gap-2">
            <Button variant="ghost" size="xs" onClick={() => setConfirmDelete(false)}>Cancel</Button>
            <Button variant="destructive" size="xs" onClick={handleDelete}>Delete</Button>
          </div>
        </div>
      )}

      {error && (
        <p className="text-xs text-destructive px-1">{error}</p>
      )}
    </div>
  );
}
