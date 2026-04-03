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
      return 'unknown';
  }
}

export default function TaskItem({ task, onEdit, onDeleted }: TaskItemProps) {
  const [isRunning, setIsRunning] = useState(false);
  const [isToggling, setIsToggling] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  async function handleToggle(checked: boolean) {
    if (isToggling) return;
    setIsToggling(true);
    try {
      await updateTask({ id: task.id, enabled: checked });
      await queryClient.invalidateQueries({ queryKey: ['tasks'] });
    } catch (err) {
      console.error('Failed to toggle task:', err);
    } finally {
      setIsToggling(false);
    }
  }

  async function handleRun() {
    if (isRunning) return;
    setIsRunning(true);
    try {
      await runTaskNow(task.id);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      alert(`Failed to run task: ${message}`);
    } finally {
      setIsRunning(false);
    }
  }

  async function handleDelete() {
    const confirmed = confirm(`Delete task "${task.name}"?`);
    if (!confirmed) return;
    setIsDeleting(true);
    try {
      await deleteTask(task.id);
      onDeleted();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      alert(`Failed to delete task: ${message}`);
      setIsDeleting(false);
    }
  }

  return (
    <div className="flex items-center gap-2 px-3 py-2 hover:bg-muted/50 group">
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

      {/* Actions — only visible on hover */}
      <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
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
          onClick={handleDelete}
          disabled={isDeleting}
          title="Delete task"
          aria-label="Delete task"
          className="text-destructive hover:text-destructive hover:bg-destructive/10"
        >
          {isDeleting ? <Loader2 className="animate-spin" /> : <Trash2 />}
        </Button>
      </div>
    </div>
  );
}
