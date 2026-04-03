import { useState } from 'react';
import TaskList from './components/TaskList';

type View = 'list' | 'add' | 'edit' | 'history';

export default function App() {
  const [view, setView] = useState<View>('list');
  const [editingId, setEditingId] = useState<string | null>(null);

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
        {(view === 'add' || view === 'edit') && (
          <div className="p-4">
            <p className="text-sm text-muted-foreground">
              {view === 'add' ? 'Create task (coming soon)' : `Edit task ${editingId}`}
            </p>
            <button onClick={() => setView('list')} className="mt-2 text-sm underline">
              Back
            </button>
          </div>
        )}
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
