import { useState } from 'react';
import { Plus, Clock } from 'lucide-react';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { queryClient } from '@/lib';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import TaskList from './components/TaskList';
import TemplateGrid, { type TemplateName } from './components/TemplateGrid';
import HistoryView from './components/HistoryView';

type View = 'list' | 'templates' | 'history';

async function openEditorWindow(params: { template?: TemplateName; taskId?: string }) {
  const query = new URLSearchParams();
  if (params.template) query.set('template', params.template);
  if (params.taskId) query.set('taskId', params.taskId);

  const label = `editor-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
  const url = `/src/editor.html?${query.toString()}`;

  try {
    const win = new WebviewWindow(label, {
      url,
      title: params.taskId ? 'Edit Task — cronmac' : 'New Task — cronmac',
      width: 500,
      height: 600,
      resizable: true,
      center: true,
      decorations: true,
    });

    // Refresh task list when editor closes
    win.once('tauri://destroyed', () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] });
    });
  } catch (err) {
    console.error('Failed to open editor window:', err);
  }
}

export default function App() {
  const [view, setView] = useState<View>('list');

  function handleTemplateSelect(template: TemplateName) {
    openEditorWindow({ template });
    setView('list');
  }

  function handleEditTask(id: string) {
    openEditorWindow({ taskId: id });
  }

  return (
    <div className="flex flex-col h-screen w-full bg-background text-foreground select-none overflow-hidden rounded-xl">
      {/* Header */}
      <div className="flex items-center justify-between px-4 h-12 shrink-0">
        <span className="text-sm font-semibold tracking-tight">cronmac</span>
        <Button variant="ghost" size="icon-sm" onClick={() => setView('templates')}>
          <Plus />
        </Button>
      </div>
      <Separator />

      {/* Content */}
      <div className="flex-1 overflow-hidden">
        {view === 'list' && (
          <TaskList
            onEdit={handleEditTask}
            onAdd={() => setView('templates')}
          />
        )}
        {view === 'templates' && (
          <TemplateGrid
            onSelect={handleTemplateSelect}
            onBack={() => setView('list')}
          />
        )}
        {view === 'history' && <HistoryView onBack={() => setView('list')} />}
      </div>

      {/* Footer */}
      <Separator />
      <div className="flex items-center justify-end px-4 h-10 shrink-0">
        <Button variant="ghost" size="sm" onClick={() => setView('history')} className="text-muted-foreground">
          <Clock />
          History
        </Button>
      </div>
    </div>
  );
}
