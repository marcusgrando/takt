import { useState } from 'react';
import { Loader2 } from 'lucide-react';
import { createTask, updateTask, type TaskDto, type Schedule, type Action } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import ScheduleBuilder from './ScheduleBuilder';
import ActionBuilder from './ActionBuilder';

export interface TaskWizardProps {
  initialTask?: TaskDto;
  onSaved: (task: TaskDto) => void;
  onCancel: () => void;
}

const STEPS = ['Details', 'Schedule', 'Action'] as const;
type Step = 0 | 1 | 2;

const DEFAULT_SCHEDULE: Schedule = { type: 'Cron', expression: '0 * * * *' };
const DEFAULT_ACTION: Action = { type: 'OpenUrl', url: '', browser: undefined };

export default function TaskWizard({ initialTask, onSaved, onCancel }: TaskWizardProps) {
  const isEdit = !!initialTask;

  const [step, setStep] = useState<Step>(0);
  const [name, setName] = useState(initialTask?.name ?? '');
  const [description, setDescription] = useState(initialTask?.description ?? '');
  const [schedule, setSchedule] = useState<Schedule>(initialTask?.schedule ?? DEFAULT_SCHEDULE);
  const [action, setAction] = useState<Action>(initialTask?.action ?? DEFAULT_ACTION);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  function canAdvance(): boolean {
    if (step === 0) return name.trim().length > 0;
    return true;
  }

  async function handleSave() {
    setSaveError(null);

    // Validate schedule
    if (schedule.type === 'Cron' && !schedule.expression.trim()) {
      setSaveError('Cron expression cannot be empty');
      return;
    }
    if (schedule.type === 'OneShot') {
      const d = new Date((schedule as { type: 'OneShot'; run_at: string }).run_at ?? '');
      if (isNaN(d.getTime())) {
        setSaveError('Please set a valid date and time');
        return;
      }
    }

    // Validate action
    if (action.type === 'OpenFile' && !(action.path?.trim())) {
      setSaveError('File path cannot be empty');
      return;
    }
    if (action.type === 'OpenUrl' && !(action.url?.trim())) {
      setSaveError('URL cannot be empty');
      return;
    }
    if (action.type === 'RunCommand' && !(action.command?.trim())) {
      setSaveError('Command cannot be empty');
      return;
    }
    if (action.type === 'Webhook' && !(action.url?.trim())) {
      setSaveError('Webhook URL cannot be empty');
      return;
    }

    setSaving(true);
    try {
      let saved: TaskDto;
      if (isEdit) {
        saved = await updateTask({
          id: initialTask!.id,
          name: name.trim(),
          description: description.trim() || null,
          schedule,
          action,
        });
      } else {
        saved = await createTask({
          name: name.trim(),
          description: description.trim() || undefined,
          schedule,
          action,
        });
      }
      onSaved(saved);
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 border-b shrink-0">
        <button
          type="button"
          onClick={onCancel}
          className="text-xs text-muted-foreground hover:text-foreground"
        >
          ← Cancel
        </button>
        <span className="flex-1 text-center text-xs font-semibold">
          {isEdit ? 'Edit task' : 'New task'}
        </span>
        {/* Step indicator */}
        <div className="flex items-center gap-1">
          {STEPS.map((_, i) => (
            <span
              key={i}
              className={[
                'size-1.5 rounded-full transition-colors',
                i === step ? 'bg-foreground' : 'bg-muted-foreground/30',
              ].join(' ')}
            />
          ))}
        </div>
      </div>

      {/* Step content — scrollable */}
      <div className="flex-1 overflow-y-auto px-3 py-3">
        <p className="text-[10px] uppercase tracking-widest text-muted-foreground mb-3">
          Step {step + 1} — {STEPS[step]}
        </p>

        {step === 0 && (
          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-1">
              <Label htmlFor="task-name" className="text-xs text-muted-foreground">
                Name <span className="text-destructive">*</span>
              </Label>
              <Input
                id="task-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="My task"
                className="h-7 text-xs"
                autoFocus
              />
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="task-desc" className="text-xs text-muted-foreground">
                Description <span className="opacity-50">(optional)</span>
              </Label>
              <Textarea
                id="task-desc"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="What does this task do?"
                className="min-h-16 text-xs resize-none"
                rows={3}
              />
            </div>
          </div>
        )}

        {step === 1 && (
          <ScheduleBuilder value={schedule} onChange={setSchedule} />
        )}

        {step === 2 && (
          <ActionBuilder value={action} onChange={setAction} />
        )}

        {saveError && (
          <div className="mt-3 flex items-start justify-between rounded bg-destructive/10 px-2 py-1.5 text-[11px] text-destructive">
            <span className="flex-1 break-words">{saveError}</span>
            <button
              type="button"
              onClick={() => setSaveError(null)}
              className="ml-2 shrink-0 opacity-60 hover:opacity-100"
              aria-label="Dismiss error"
            >
              ✕
            </button>
          </div>
        )}
      </div>

      {/* Footer nav */}
      <div className="flex items-center justify-between px-3 py-2 border-t shrink-0">
        <Button
          type="button"
          variant="ghost"
          size="xs"
          onClick={() => setStep((s) => (s - 1) as Step)}
          disabled={step === 0}
        >
          Back
        </Button>

        {step < 2 ? (
          <Button
            type="button"
            size="xs"
            onClick={() => setStep((s) => (s + 1) as Step)}
            disabled={!canAdvance()}
          >
            Next
          </Button>
        ) : (
          <Button
            type="button"
            size="xs"
            onClick={handleSave}
            disabled={saving}
          >
            {saving && <Loader2 className="animate-spin" />}
            {saving ? 'Saving…' : isEdit ? 'Save' : 'Create'}
          </Button>
        )}
      </div>
    </div>
  );
}
