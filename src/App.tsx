import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { queryClient } from '@/lib';
import { listTasks } from '@/lib/api';
import TaskList from './components/TaskList';
import TaskWizard from './components/TaskWizard';

type View = 'list' | 'add' | 'edit' | 'history';

export default function App() {
  const [view, setView] = useState<View>('list');
  const [editingId, setEditingId] = useState<string | null>(null);

  const { data: tasks } = useQuery({
    queryKey: ['tasks'],
    queryFn: listTasks,
    staleTime: 30_000,
  });

  return (
    <div className="flex flex-col h-screen w-full bg-background text-foreground select-none">
      {/* Header */}
      <div className="flex items-center justify-between px-3 h-10 border-b shrink-0">
        <span className="text-sm font-semibold">cronmac</span>
        <button
          onClick={() => setView('add')}
          className="text-muted-foreground hover:text-foreground text-lg leading-none"
          title="Add task"
        >
          +
        </button>
      </div>

      {/* Main content */}
      <div className="flex-1 overflow-hidden">
        {view === 'list' && (
          <TaskList
            onEdit={(id) => { setEditingId(id); setView('edit'); }}
            onAdd={() => setView('add')}
          />
        )}

        {view === 'add' && (
          <TaskWizard
            onSaved={(_task) => {
              queryClient.invalidateQueries({ queryKey: ['tasks'] });
              setView('list');
            }}
            onCancel={() => setView('list')}
          />
        )}

        {view === 'edit' && (() => {
          const editTask = tasks?.find((t) => t.id === editingId);
          if (!editTask) {
            return (
              <div className="flex flex-col items-center justify-center h-full gap-2">
                <div className="text-sm text-muted-foreground">Loading…</div>
                <button
                  onClick={() => setView('list')}
                  className="text-xs text-muted-foreground hover:text-foreground underline"
                >
                  Cancel
                </button>
              </div>
            );
          }
          return (
            <TaskWizard
              initialTask={editTask}
              onSaved={(_task) => {
                queryClient.invalidateQueries({ queryKey: ['tasks'] });
                setView('list');
              }}
              onCancel={() => setView('list')}
            />
          );
        })()}

        {view === 'history' && (
          <div className="p-4">
            <button onClick={() => setView('list')} className="text-xs text-muted-foreground hover:text-foreground mb-2">
              ← Back
            </button>
            <p className="text-sm text-muted-foreground">History (coming soon)</p>
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="flex items-center justify-end px-3 h-8 border-t shrink-0">
        <button
          onClick={() => setView('history')}
          className="text-xs text-muted-foreground hover:text-foreground"
        >
          History
        </button>
      </div>
    </div>
  );
}
