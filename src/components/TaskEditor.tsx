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
