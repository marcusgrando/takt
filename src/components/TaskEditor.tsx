// src/components/TaskEditor.tsx
import { useState, useEffect, useRef, useMemo } from 'react';
import { Loader2, Trash2 } from 'lucide-react';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { confirm } from '@tauri-apps/plugin-dialog';
import { createTask, updateTask, deleteTask, type TaskDto, type Schedule, type Action } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Separator } from '@/components/ui/separator';
import { Switch } from '@/components/ui/switch';
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
  const [runIfMissed, setRunIfMissed] = useState(task?.run_if_missed ?? true);
  const [notifyOnRun, setNotifyOnRun] = useState(task?.notify_on_run ?? false);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const closingRef = useRef(false);

  // Snapshot initial values for dirty comparison
  const initialSnapshot = useRef({
    name: task?.name ?? '',
    description: task?.description ?? '',
    schedule: JSON.stringify(initialSchedule),
    action: JSON.stringify(initialAction),
    runIfMissed: task?.run_if_missed ?? true,
    notifyOnRun: task?.notify_on_run ?? false,
  });

  // Auto-name generation
  useEffect(() => {
    if (!nameManual) {
      setName(generateAutoName(action, schedule));
    }
  }, [action, schedule, nameManual]);

  // Compute dirty by comparing current vs initial (no effects needed)
  const dirty = useMemo(() => {
    const snap = initialSnapshot.current;
    const currentName = nameManual ? name : generateAutoName(action, schedule);
    const initialName = isEdit ? snap.name : generateAutoName(
      JSON.parse(snap.action) as Action,
      JSON.parse(snap.schedule) as Schedule,
    );
    if (currentName !== initialName) return true;
    if (description !== snap.description) return true;
    if (JSON.stringify(schedule) !== snap.schedule) return true;
    if (JSON.stringify(action) !== snap.action) return true;
    if (runIfMissed !== snap.runIfMissed) return true;
    if (notifyOnRun !== snap.notifyOnRun) return true;
    return false;
  }, [name, nameManual, description, schedule, action, runIfMissed, notifyOnRun, isEdit]);

  const dirtyRef = useRef(false);
  dirtyRef.current = dirty;

  // ESC closes the editor window (triggers close-requested, so dirty check applies)
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        getCurrentWebviewWindow().close();
      }
    }
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // Intercept window close — show native confirmation if dirty
  const unlistenRef = useRef<(() => void) | null>(null);
  useEffect(() => {
    const win = getCurrentWebviewWindow();
    const promise = win.onCloseRequested(async (event) => {
      if (dirtyRef.current && !closingRef.current) {
        event.preventDefault();
        const ok = await confirm('You have unsaved changes. Close without saving?', {
          title: 'cronmac',
          kind: 'warning',
        });
        if (ok) {
          closingRef.current = true;
          // Remove listener before closing to avoid re-entry
          unlistenRef.current?.();
          await win.close();
        }
      }
    });
    promise.then((fn) => { unlistenRef.current = fn; });
    return () => { promise.then((fn) => fn()); };
  }, []);

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
    if (action.type === 'OpenApp' && !action.app_path?.trim()) { setError('Application path is required'); return; }
    if (action.type === 'RunCommand' && !action.command?.trim()) { setError('Command is required'); return; }
    if (action.type === 'Notify' && !action.title?.trim()) { setError('Notification title is required'); return; }
    if (action.type === 'Webhook' && !action.url?.trim()) { setError('Webhook URL is required'); return; }

    setSaving(true);
    try {
      if (isEdit) {
        await updateTask({ id: task!.id, name: name.trim(), description: description.trim() || null, run_if_missed: runIfMissed, notify_on_run: notifyOnRun, schedule, action });
      } else {
        await createTask({ name: name.trim(), description: description.trim() || undefined, run_if_missed: runIfMissed, notify_on_run: notifyOnRun, schedule, action });
      }
      closingRef.current = true; // allow window to close without prompt
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
    closingRef.current = true;
    try {
      await deleteTask(task.id);
      await onSaved?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setDeleting(false);
      setConfirmDelete(false);
      closingRef.current = false;
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
      <div className="flex-1 overflow-y-auto px-5 py-5 pb-10 space-y-6 scrollbar-none">
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

        {/* Run if missed */}
        <div className="flex items-center justify-between">
          <div>
            <Label>Run if missed</Label>
            <p className="text-xs text-muted-foreground">Execute on wake if a run was missed while inactive</p>
          </div>
          <Switch checked={runIfMissed} onCheckedChange={setRunIfMissed} />
        </div>

        {/* Notify on run */}
        <div className="flex items-center justify-between">
          <div>
            <Label>Notify on run</Label>
            <p className="text-xs text-muted-foreground">Show a notification when the task executes</p>
          </div>
          <Switch checked={notifyOnRun} onCheckedChange={setNotifyOnRun} />
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
