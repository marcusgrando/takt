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

function getActionLabel(action: Action): string {
  switch (action.type) {
    case 'RunCommand':
      return 'shell';
    case 'OpenUrl':
      return 'url';
    case 'Notify':
      return 'notify';
    case 'OpenFile':
      return 'file';
    case 'Shortcut':
      return 'shortcut';
    case 'Webhook':
      return 'webhook';
    default:
      (action as never) satisfies never;
      return 'unknown';
  }
}

export default function TaskItem({ task, onEdit, onDeleted }: TaskItemProps) {
  const [isRunning, setIsRunning] = useState(false);
  const [isToggling, setIsToggling] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [opError, setOpError] = useState<string | null>(null);

  async function handleToggle(checked: boolean) {
    if (isToggling) return;
    // Optimistic update
    queryClient.setQueryData<TaskDto[]>(['tasks'], (prev) =>
      prev?.map((t) => t.id === task.id ? { ...t, enabled: checked } : t) ?? []
    );
    try {
      setIsToggling(true);
      await updateTask({ id: task.id, enabled: checked });
      await queryClient.invalidateQueries({ queryKey: ['tasks'] });
    } catch (err) {
      // Revert on failure
      queryClient.setQueryData<TaskDto[]>(['tasks'], (prev) =>
        prev?.map((t) => t.id === task.id ? { ...t, enabled: task.enabled } : t) ?? []
      );
      setOpError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsToggling(false);
    }
  }

  async function handleRun() {
    if (isRunning) return;
    setOpError(null);
    setIsRunning(true);
    try {
      await runTaskNow(task.id);
    } catch (err) {
      setOpError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsRunning(false);
    }
  }

  async function handleDeleteConfirmed() {
    setIsDeleting(true);
    setConfirmingDelete(false);
    try {
      await deleteTask(task.id);
      setIsDeleting(false);
      onDeleted();
    } catch (err) {
      setOpError(err instanceof Error ? err.message : String(err));
      setIsDeleting(false);
    }
  }

  return (
    <div className="flex flex-col px-3 py-2 hover:bg-muted/50 group">
      <div className="flex items-center gap-2">
        {/* Toggle */}
        <Switch
          checked={task.enabled}
          onCheckedChange={handleToggle}
          disabled={isToggling}
          size="sm"
          aria-label={`Toggle ${task.name}`}
        />

        {/* Name + badge */}
        <div className="flex flex-col min-w-0 flex-1">
          <span
            className="text-xs font-medium truncate leading-tight"
            title={task.name}
          >
            {task.name}
          </span>
          <Badge
            variant="secondary"
            className="mt-0.5 w-fit text-[10px] h-4 px-1.5"
          >
            {getActionLabel(task.action)}
          </Badge>
        </div>

        {/* Actions — visible on hover or keyboard focus within */}
        <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity">
          {/* Play */}
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={handleRun}
            disabled={isRunning}
            title="Run now"
            aria-label="Run task now"
          >
            {isRunning ? (
              <Loader2 className="animate-spin" />
            ) : (
              <Play />
            )}
          </Button>

          {/* Edit */}
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={() => onEdit(task.id)}
            title="Edit task"
            aria-label="Edit task"
          >
            <Pencil />
          </Button>

          {/* Delete */}
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={() => setConfirmingDelete(true)}
            disabled={isDeleting}
            title="Delete task"
            aria-label="Delete task"
            className="text-destructive hover:text-destructive hover:bg-destructive/10"
          >
            {isDeleting ? <Loader2 className="animate-spin" /> : <Trash2 />}
          </Button>
        </div>
      </div>

      {/* Inline delete confirmation */}
      {confirmingDelete && (
        <div className="flex items-center justify-between mt-1.5 px-1 py-1 rounded bg-destructive/10 text-xs">
          <span className="text-destructive">Delete this task?</span>
          <div className="flex gap-1">
            <Button
              variant="ghost"
              size="xs"
              onClick={() => setConfirmingDelete(false)}
              className="h-5 px-2 text-xs"
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              size="xs"
              onClick={handleDeleteConfirmed}
              className="h-5 px-2 text-xs"
            >
              Delete
            </Button>
          </div>
        </div>
      )}

      {/* Inline error message */}
      {opError && (
        <div className="flex items-center justify-between mt-1 px-1 text-[10px] text-destructive">
          <span className="truncate">{opError}</span>
          <button
            onClick={() => setOpError(null)}
            className="ml-1 shrink-0 opacity-60 hover:opacity-100"
            aria-label="Dismiss error"
          >
            ✕
          </button>
        </div>
      )}
    </div>
  );
}
