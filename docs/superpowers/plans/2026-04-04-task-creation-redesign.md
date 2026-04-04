# Task Creation Flow Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 3-step wizard with a template-launcher popover + full editor window, fixing contrast issues along the way.

**Architecture:** Popover shows task list + template grid (no forms). Clicking a template or existing task opens a second Tauri webview window (`editor`) with a single-page form. Both windows share the same Tauri backend. Communication via URL query params.

**Tech Stack:** Tauri v2 (Rust + WebviewWindow JS API), React 19, TanStack Query, shadcn/ui (radix-vega/stone), Tailwind v4, Lucide icons.

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `src/components/TemplateGrid.tsx` | Template cards grid shown in popover on "+" click |
| Create | `src/components/TaskEditor.tsx` | Full editor form (single-page, all sections) |
| Create | `src/components/AutoName.ts` | Pure function: `generateAutoName(action, schedule) → string` |
| Create | `src/Editor.tsx` | Entry point for the editor window (reads query params, renders TaskEditor) |
| Create | `src/editor.html` | HTML entry point for the editor Tauri window |
| Create | `src/editor-main.tsx` | React root bootstrap for editor window |
| Modify | `src/App.tsx` | Replace wizard view states with template grid; add `openEditorWindow()` |
| Modify | `src/index.css` | Fix contrast: TabsList, TabsTrigger, preset buttons |
| Modify | `src-tauri/src/lib.rs` | Register the `editor.html` as a valid Tauri window source |
| Modify | `src-tauri/tauri.conf.json` | Add security config for second window if needed |
| Modify | `vite.config.ts` | Add `editor.html` as a multi-page entry point |
| Delete | `src/components/TaskWizard.tsx` | Replaced by TemplateGrid + TaskEditor |

---

### Task 1: Auto-Name Generator

**Files:**
- Create: `src/components/AutoName.ts`

- [ ] **Step 1: Create the auto-name generator**

This is a pure function with no dependencies. It generates human-readable names from action + schedule data.

```ts
// src/components/AutoName.ts
import type { Action, Schedule } from '@/lib/api';

function describeAction(action: Action): string {
  switch (action.type) {
    case 'OpenUrl': {
      if (!action.url) return 'Open URL';
      try {
        const host = new URL(action.url).hostname.replace(/^www\./, '');
        return `Open ${host}`;
      } catch {
        return `Open ${action.url.slice(0, 30)}`;
      }
    }
    case 'OpenFile': {
      if (!action.path) return 'Open file';
      const name = action.path.split('/').pop() || action.path;
      return `Open ${name}`;
    }
    case 'RunCommand': {
      if (!action.command) return 'Run command';
      const cmd = action.command.split(/\s/)[0].split('/').pop() || action.command;
      return `Run ${cmd}`;
    }
    case 'Notify': {
      if (!action.title) return 'Reminder';
      return `Reminder: ${action.title}`;
    }
    case 'Shortcut': {
      if (action.keys.length === 0) return 'Shortcut';
      return `Shortcut ${action.keys.join('+')}`;
    }
    case 'Webhook': {
      if (!action.url) return 'Webhook';
      try {
        const host = new URL(action.url).hostname.replace(/^www\./, '');
        return `${action.method} ${host}`;
      } catch {
        return `${action.method} webhook`;
      }
    }
  }
}

function describeSchedule(schedule: Schedule): string {
  switch (schedule.type) {
    case 'OnLogin': return 'On login';
    case 'OnWake': return 'On wake';
    case 'OneShot': {
      const d = new Date(schedule.run_at);
      if (isNaN(d.getTime())) return 'One time';
      return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
    }
    case 'Cron': {
      const expr = schedule.expression;
      const presets: Record<string, string> = {
        '* * * * *': 'Every minute',
        '*/5 * * * *': 'Every 5 min',
        '0 * * * *': 'Every hour',
        '0 0 * * *': 'Daily midnight',
        '0 9 * * *': 'Daily 9am',
        '0 0 * * 1': 'Weekly Monday',
      };
      return presets[expr] ?? `Cron ${expr}`;
    }
  }
}

export function generateAutoName(action: Action, schedule: Schedule): string {
  return `${describeAction(action)} — ${describeSchedule(schedule)}`;
}
```

- [ ] **Step 2: Commit**

```bash
git add src/components/AutoName.ts
git commit -m "feat: add auto-name generator for tasks"
```

---

### Task 2: Fix Contrast — CSS and Tab Components

**Files:**
- Modify: `src/index.css`

- [ ] **Step 1: Fix TabsList contrast in CSS variables**

The problem is `--muted: oklch(0.97 ...)` is nearly indistinguishable from `--background: oklch(1 0 0)`. We fix this by making `--muted` slightly darker for better contrast in light mode. Also fix `--secondary` to be a clear button background.

In `src/index.css`, inside the `:root` block, change:

```css
/* old */
--muted: oklch(0.97 0.001 106.424);
--secondary: oklch(0.967 0.001 286.375);
```

to:

```css
/* new — more visible separation from white background */
--muted: oklch(0.94 0.002 106.424);
--secondary: oklch(0.945 0.002 286.375);
```

- [ ] **Step 2: Verify visually**

Run: `cd /Users/marcus.grando/git/cronmac && bun run dev`

Open the app and confirm that:
- TabsList backgrounds are visibly distinct from page background
- Active tab triggers have clear white background against the TabsList
- Buttons with `variant="outline"` have visible borders

- [ ] **Step 3: Commit**

```bash
git add src/index.css
git commit -m "fix: improve contrast for muted/secondary color tokens"
```

---

### Task 3: Template Grid Component

**Files:**
- Create: `src/components/TemplateGrid.tsx`

- [ ] **Step 1: Create TemplateGrid**

This component replaces the wizard in the popover. It shows a 2-column grid of template cards. Clicking one calls `onSelect` with the template type.

```tsx
// src/components/TemplateGrid.tsx
import { Link, FileText, Terminal, Bell, Globe, Settings } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { Action, Schedule } from '@/lib/api';

export type TemplateName = 'OpenUrl' | 'OpenFile' | 'RunCommand' | 'Notify' | 'Webhook' | 'Custom';

export interface TemplateConfig {
  name: TemplateName;
  label: string;
  icon: React.ReactNode;
  action: Action;
  schedule: Schedule;
}

export const TEMPLATES: TemplateConfig[] = [
  {
    name: 'OpenUrl',
    label: 'Open URL',
    icon: <Link className="size-5" />,
    action: { type: 'OpenUrl', url: '', browser: undefined },
    schedule: { type: 'Cron', expression: '0 9 * * *' },
  },
  {
    name: 'OpenFile',
    label: 'Open File',
    icon: <FileText className="size-5" />,
    action: { type: 'OpenFile', path: '' },
    schedule: { type: 'Cron', expression: '0 9 * * *' },
  },
  {
    name: 'RunCommand',
    label: 'Run Command',
    icon: <Terminal className="size-5" />,
    action: { type: 'RunCommand', command: '', args: [], shell: 'Zsh' },
    schedule: { type: 'Cron', expression: '0 * * * *' },
  },
  {
    name: 'Notify',
    label: 'Reminder',
    icon: <Bell className="size-5" />,
    action: { type: 'Notify', title: '', body: '', sound: true },
    schedule: { type: 'Cron', expression: '0 9 * * *' },
  },
  {
    name: 'Webhook',
    label: 'Webhook',
    icon: <Globe className="size-5" />,
    action: { type: 'Webhook', url: '', method: 'GET', headers: {}, body: undefined },
    schedule: { type: 'Cron', expression: '0 * * * *' },
  },
  {
    name: 'Custom',
    label: 'Custom',
    icon: <Settings className="size-5" />,
    action: { type: 'OpenUrl', url: '', browser: undefined },
    schedule: { type: 'Cron', expression: '0 9 * * *' },
  },
];

interface TemplateGridProps {
  onSelect: (template: TemplateName) => void;
  onBack: () => void;
}

export default function TemplateGrid({ onSelect, onBack }: TemplateGridProps) {
  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center px-4 h-10 shrink-0">
        <Button variant="link" size="sm" onClick={onBack} className="px-0 text-primary">
          Back
        </Button>
        <span className="flex-1 text-center text-sm font-semibold">New Task</span>
        <div className="w-10" /> {/* spacer for centering */}
      </div>
      <div className="flex-1 overflow-y-auto px-4 py-3 scrollbar-none">
        <div className="grid grid-cols-2 gap-2">
          {TEMPLATES.map((tpl) => (
            <button
              key={tpl.name}
              onClick={() => onSelect(tpl.name)}
              className="flex flex-col items-center gap-2 rounded-lg border border-border bg-background p-4 text-sm font-medium transition-colors hover:bg-muted hover:border-primary/30 active:bg-muted/80"
            >
              <span className="text-muted-foreground">{tpl.icon}</span>
              {tpl.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add src/components/TemplateGrid.tsx
git commit -m "feat: add template grid component for popover"
```

---

### Task 4: Vite Multi-Page + Editor Entry Point

**Files:**
- Create: `src/editor.html`
- Create: `src/editor-main.tsx`
- Modify: `vite.config.ts`

- [ ] **Step 1: Read current vite.config.ts**

Read the file to understand the current config before modifying.

- [ ] **Step 2: Create editor HTML entry point**

```html
<!-- src/editor.html -->
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>cronmac — Editor</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/editor-main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 3: Create editor React bootstrap**

```tsx
// src/editor-main.tsx
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClientProvider } from '@tanstack/react-query';
import { queryClient } from '@/lib';
import Editor from './Editor';
import './index.css';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <Editor />
    </QueryClientProvider>
  </StrictMode>,
);
```

- [ ] **Step 4: Add editor to Vite multi-page config**

In `vite.config.ts`, add the `build.rollupOptions.input` to register both entry points:

```ts
// vite.config.ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { resolve } from "path";

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      "@": resolve(__dirname, "./src"),
    },
  },

  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        editor: resolve(__dirname, "src/editor.html"),
      },
    },
  },

  // Vite options tailored for Tauri development
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
```

- [ ] **Step 5: Commit**

```bash
git add src/editor.html src/editor-main.tsx vite.config.ts
git commit -m "feat: add Vite multi-page setup for editor window"
```

---

### Task 5: TaskEditor Component

**Files:**
- Create: `src/components/TaskEditor.tsx`

This is the main form for the full editor window. Single page, sections for Action, Schedule, Name, Description. Reuses existing `ScheduleBuilder` and `ActionBuilder` components.

- [ ] **Step 1: Create TaskEditor**

```tsx
// src/components/TaskEditor.tsx
import { useState, useEffect } from 'react';
import { Loader2, Trash2 } from 'lucide-react';
import { createTask, updateTask, deleteTask, type TaskDto, type Schedule, type Action } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Separator } from '@/components/ui/separator';
import ScheduleBuilder from './ScheduleBuilder';
import ActionBuilder from './ActionBuilder';
import { generateAutoName } from './AutoName';
import { type TemplateName, TEMPLATES } from './TemplateGrid';

export interface TaskEditorProps {
  /** Edit an existing task */
  task?: TaskDto;
  /** Create from a template */
  template?: TemplateName;
  /** Called after successful save */
  onSaved?: () => void;
}

export default function TaskEditor({ task, template, onSaved }: TaskEditorProps) {
  const isEdit = !!task;

  // Resolve initial values from task or template
  const tpl = template ? TEMPLATES.find((t) => t.name === template) : undefined;
  const initialAction = task?.action ?? tpl?.action ?? TEMPLATES[0].action;
  const initialSchedule = task?.schedule ?? tpl?.schedule ?? TEMPLATES[0].schedule;

  const [name, setName] = useState(task?.name ?? '');
  const [nameManual, setNameManual] = useState(isEdit); // edit mode = manual by default
  const [description, setDescription] = useState(task?.description ?? '');
  const [showDescription, setShowDescription] = useState(!!task?.description);
  const [schedule, setSchedule] = useState<Schedule>(initialSchedule);
  const [action, setAction] = useState<Action>(initialAction);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Auto-name generation
  useEffect(() => {
    if (!nameManual) {
      setName(generateAutoName(action, schedule));
    }
  }, [action, schedule, nameManual]);

  function handleNameChange(value: string) {
    setNameManual(true);
    setName(value);
  }

  async function handleSave() {
    setError(null);
    // Validation
    if (schedule.type === 'Cron' && !schedule.expression.trim()) { setError('Cron expression is required'); return; }
    if (schedule.type === 'OneShot' && isNaN(new Date(schedule.run_at).getTime())) { setError('Invalid date'); return; }
    if (action.type === 'OpenFile' && !action.path?.trim()) { setError('File path is required'); return; }
    if (action.type === 'OpenUrl' && !action.url?.trim()) { setError('URL is required'); return; }
    if (action.type === 'RunCommand' && !action.command?.trim()) { setError('Command is required'); return; }
    if (action.type === 'Webhook' && !action.url?.trim()) { setError('Webhook URL is required'); return; }

    setSaving(true);
    try {
      if (isEdit) {
        await updateTask({ id: task!.id, name: name.trim(), description: description.trim() || null, schedule, action });
      } else {
        await createTask({ name: name.trim(), description: description.trim() || undefined, schedule, action });
      }
      onSaved?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    if (!task) return;
    setDeleting(true);
    try {
      await deleteTask(task.id);
      onSaved?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setDeleting(false);
      setConfirmDelete(false);
    }
  }

  return (
    <div className="flex flex-col h-screen bg-background text-foreground">
      {/* Title bar area */}
      <div className="flex items-center justify-between px-5 h-14 shrink-0 border-b border-border">
        <span className="text-sm font-semibold">{isEdit ? 'Edit Task' : 'New Task'}</span>
        <Button size="sm" onClick={handleSave} disabled={saving}>
          {saving && <Loader2 className="animate-spin" />}
          {saving ? 'Saving…' : isEdit ? 'Save' : 'Create'}
        </Button>
      </div>

      {/* Scrollable form */}
      <div className="flex-1 overflow-y-auto px-5 py-5 space-y-6 scrollbar-none">
        {/* Name */}
        <div className="space-y-2">
          <Label htmlFor="task-name">Name</Label>
          <Input
            id="task-name"
            value={name}
            onChange={(e) => handleNameChange(e.target.value)}
            placeholder="Auto-generated from action + schedule"
            className={!nameManual ? 'text-muted-foreground' : ''}
          />
          {nameManual && (
            <button
              type="button"
              onClick={() => setNameManual(false)}
              className="text-xs text-primary hover:underline"
            >
              Reset to auto-name
            </button>
          )}
        </div>

        {/* Action */}
        <div className="space-y-3">
          <Label className="text-xs font-medium uppercase tracking-widest text-muted-foreground">Action</Label>
          <ActionBuilder value={action} onChange={setAction} />
        </div>

        <Separator />

        {/* Schedule */}
        <div className="space-y-3">
          <Label className="text-xs font-medium uppercase tracking-widest text-muted-foreground">Schedule</Label>
          <ScheduleBuilder value={schedule} onChange={setSchedule} />
        </div>

        <Separator />

        {/* Description (collapsible) */}
        {!showDescription ? (
          <button
            type="button"
            onClick={() => setShowDescription(true)}
            className="text-sm text-primary hover:underline"
          >
            + Add description
          </button>
        ) : (
          <div className="space-y-2">
            <Label htmlFor="task-desc">Description</Label>
            <Textarea
              id="task-desc"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Optional notes about this task"
              rows={3}
              className="resize-none"
            />
          </div>
        )}

        {/* Error */}
        {error && (
          <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
        )}

        {/* Delete (edit mode only) */}
        {isEdit && (
          <div className="pt-2">
            {confirmDelete ? (
              <div className="flex items-center justify-between rounded-md bg-destructive/10 px-3 py-2">
                <span className="text-sm text-destructive font-medium">Delete this task?</span>
                <div className="flex gap-2">
                  <Button variant="ghost" size="sm" onClick={() => setConfirmDelete(false)}>Cancel</Button>
                  <Button variant="destructive" size="sm" onClick={handleDelete} disabled={deleting}>
                    {deleting && <Loader2 className="animate-spin" />}
                    Delete
                  </Button>
                </div>
              </div>
            ) : (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setConfirmDelete(true)}
                className="text-destructive hover:text-destructive hover:bg-destructive/10"
              >
                <Trash2 className="size-4" />
                Delete task
              </Button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add src/components/TaskEditor.tsx
git commit -m "feat: add TaskEditor single-page form component"
```

---

### Task 6: Editor Window Entry Component

**Files:**
- Create: `src/Editor.tsx`

- [ ] **Step 1: Create Editor entry component**

This reads URL query params to determine create vs. edit mode, fetches the task if editing, and renders TaskEditor.

```tsx
// src/Editor.tsx
import { useQuery } from '@tanstack/react-query';
import { Loader2 } from 'lucide-react';
import { getTask } from '@/lib/api';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
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

  async function handleSaved() {
    // Close the editor window after save
    const win = getCurrentWebviewWindow();
    await win.close();
  }

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
    <TaskEditor
      task={task ?? undefined}
      template={template ?? undefined}
      onSaved={handleSaved}
    />
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add src/Editor.tsx
git commit -m "feat: add Editor entry component with query param routing"
```

---

### Task 7: Update Popover — Replace Wizard with Template Grid + Window Opener

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Read current App.tsx for exact content**

Read `src/App.tsx` to get the exact strings for editing.

- [ ] **Step 2: Rewrite App.tsx**

Replace the wizard-based views with template grid + editor window opening. The new view type is `'list' | 'templates' | 'history'` (no more `'add'` or `'edit'`).

Replace the full content of `src/App.tsx`:

```tsx
// src/App.tsx
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

let editorCounter = 0;

async function openEditorWindow(params: { template?: TemplateName; taskId?: string }) {
  const query = new URLSearchParams();
  if (params.template) query.set('template', params.template);
  if (params.taskId) query.set('taskId', params.taskId);

  const label = `editor-${++editorCounter}`;
  const url = `/src/editor.html?${query.toString()}`;

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
```

- [ ] **Step 3: Commit**

```bash
git add src/App.tsx
git commit -m "feat: replace wizard with template grid + editor window"
```

---

### Task 8: Register Editor Window in Tauri Backend

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Read current tauri.conf.json**

Read the file to get exact content.

- [ ] **Step 2: Add editor window pattern to security config**

The Tauri v2 security model requires the webview window URLs to be within the app's frontend dist. Since both `index.html` and `src/editor.html` are built by Vite into `dist/`, this should work without changes to the Rust code — Tauri's `WebviewWindow` JS API can create windows pointing to any path within the frontend dist.

However, we need to verify the `security.csp` allows it. Current config has `"csp": null` which means no restrictions — this is fine.

No changes needed to `tauri.conf.json` or `lib.rs` for the security config. The JS-side `WebviewWindow` constructor handles window creation directly.

- [ ] **Step 3: Verify the editor window URL path**

The Vite multi-page build outputs `dist/src/editor.html`. In Tauri, the frontend dist root is `../dist`. So the URL in `WebviewWindow` should be `/src/editor.html`. This matches what we set in Task 7.

Run: `cd /Users/marcus.grando/git/cronmac && bun run build`

Verify that `dist/src/editor.html` exists in the output.

If Vite outputs it at a different path, update the `url` in `openEditorWindow()` in `src/App.tsx` accordingly.

- [ ] **Step 4: Commit if any changes were needed**

```bash
git add -A && git commit -m "fix: adjust editor window URL for Vite build output"
```

---

### Task 9: Delete TaskWizard + Cleanup

**Files:**
- Delete: `src/components/TaskWizard.tsx`

- [ ] **Step 1: Remove TaskWizard**

```bash
rm src/components/TaskWizard.tsx
```

- [ ] **Step 2: Verify no remaining imports**

Search for any remaining references to TaskWizard:

```bash
grep -r "TaskWizard" src/
```

If any found, remove them.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "chore: remove TaskWizard (replaced by TemplateGrid + TaskEditor)"
```

---

### Task 10: Integration Test — Full Flow

- [ ] **Step 1: Build and run the app**

```bash
cd /Users/marcus.grando/git/cronmac && bun run dev
```

In a separate terminal:

```bash
cd /Users/marcus.grando/git/cronmac && cargo tauri dev
```

- [ ] **Step 2: Test popover flow**

1. Click tray icon → popover opens with task list
2. Click "+" → template grid appears with 6 cards
3. Click "Back" → returns to task list
4. Click "+" again → click "Open URL" template
5. Verify: editor window opens centered, with URL field focused
6. Verify: name shows auto-generated text (e.g., "Open URL — Daily 9am")

- [ ] **Step 3: Test editor create flow**

1. In the editor, type `https://google.com` in URL field
2. Verify: name auto-updates to "Open google.com — Daily 9am"
3. Click "Create"
4. Verify: editor window closes
5. Verify: popover task list shows the new task

- [ ] **Step 4: Test editor edit flow**

1. In popover, click the task name/row → editor window opens in edit mode
2. Verify: all fields populated correctly
3. Change the schedule to "Every hour"
4. Click "Save"
5. Verify: editor closes, list updates

- [ ] **Step 5: Test contrast**

1. Verify TabsList backgrounds are clearly distinct from page background
2. Verify active tab trigger is visually obvious (white on darker bg)
3. Verify schedule preset buttons have clear active/inactive states

- [ ] **Step 6: Final commit if any fixes needed**

```bash
git add -A && git commit -m "fix: integration test adjustments"
```
