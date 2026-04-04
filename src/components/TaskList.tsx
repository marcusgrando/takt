import { useQuery } from '@tanstack/react-query';
import { listTasks } from '@/lib/api';
import { queryClient } from '@/lib';
import TaskItem from './TaskItem';
import { CalendarClock, Loader2 } from 'lucide-react';
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
      <div className="flex items-center justify-center h-full gap-2 text-muted-foreground">
        <Loader2 className="size-4 animate-spin" />
        <span className="text-sm">Loading tasks…</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3 p-6">
        <p className="text-sm text-destructive">
          {error instanceof Error ? error.message : 'Failed to load tasks'}
        </p>
        <Button variant="outline" size="sm" onClick={() => refetch()}>Retry</Button>
      </div>
    );
  }

  if (!tasks || tasks.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-4 p-6">
        <CalendarClock className="size-10 text-muted-foreground/30" strokeWidth={1.5} />
        <div className="text-center space-y-1">
          <p className="text-sm font-medium">No tasks yet</p>
          <p className="text-sm text-muted-foreground">Schedule your first automation.</p>
        </div>
        <Button size="sm" onClick={onAdd}>New Task</Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col overflow-y-auto h-full scrollbar-none divide-y divide-border">
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
