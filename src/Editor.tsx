import { useQuery } from '@tanstack/react-query';
import { Loader2 } from 'lucide-react';
import { getTask } from '@/lib/api';
import TaskEditor from '@/components/TaskEditor';
import type { TemplateName } from '@/components/TemplateGrid';

const params = new URLSearchParams(window.location.search);
const taskId = params.get('taskId');
const template = params.get('template') as TemplateName | null;

export default function Editor() {
  const { data: task, isLoading, error } = useQuery({
    queryKey: ['task', taskId],
    queryFn: () => getTask(taskId!),
    enabled: !!taskId,
  });

  if (taskId && isLoading) {
    return (
      <div className="flex items-center justify-center h-screen gap-2 text-muted-foreground">
        <Loader2 className="size-4 animate-spin" />
        <span className="text-sm">Loading…</span>
      </div>
    );
  }

  if (taskId && error) {
    return (
      <div className="flex items-center justify-center h-screen">
        <p className="text-sm text-destructive">
          {error instanceof Error ? error.message : 'Failed to load task'}
        </p>
      </div>
    );
  }

  return (
    <TaskEditor task={task ?? undefined} template={template ?? undefined} />
  );
}
