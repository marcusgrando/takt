import { useQuery } from '@tanstack/react-query';
import { listTasks } from '@/lib/api';
import { queryClient } from '@/lib';
import TaskItem from './TaskItem';
import { Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';

interface TaskListProps {
  onEdit: (id: string) => void;
  onAdd: () => void;
}

export default function TaskList({ onEdit, onAdd }: TaskListProps) {
  const { data: tasks, isLoading, error, refetch } = useQuery({
    queryKey: ['tasks'],
    queryFn: listTasks,
    staleTime: 30_000,
  });

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        <Loader2 className="size-4 animate-spin mr-2" />
        <span className="text-xs">Loading tasks…</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-2 p-4 text-center">
        <p className="text-xs text-destructive">
          {error instanceof Error ? error.message : 'Failed to load tasks'}
        </p>
        <Button
          variant="outline"
          size="xs"
          onClick={() => refetch()}
        >
          Retry
        </Button>
      </div>
    );
  }

  if (!tasks || tasks.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-2 p-4 text-center">
        <p className="text-xs text-muted-foreground">No tasks yet.</p>
        <Button variant="outline" size="xs" onClick={onAdd}>
          + Add one
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col overflow-y-auto h-full py-1">
      {tasks.map((task) => (
        <TaskItem
          key={task.id}
          task={task}
          onEdit={onEdit}
          onDeleted={() => queryClient.invalidateQueries({ queryKey: ['tasks'] })}
        />
      ))}
    </div>
  );
}
