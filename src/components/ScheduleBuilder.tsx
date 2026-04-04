import { type Schedule } from '@/lib/api';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';

interface ScheduleBuilderProps {
  value: Schedule;
  onChange: (s: Schedule) => void;
}

type CronPreset = 'every-minute' | 'every-5min' | 'every-hour' | 'every-day' | 'weekly' | 'custom';

const CRON_PRESETS: { id: CronPreset; label: string; expr: string }[] = [
  { id: 'every-minute', label: 'Every minute', expr: '* * * * *' },
  { id: 'every-5min',   label: 'Every 5 min',  expr: '*/5 * * * *' },
  { id: 'every-hour',   label: 'Every hour',   expr: '0 * * * *' },
  { id: 'every-day',    label: 'Every day',    expr: '0 0 * * *' },
  { id: 'weekly',       label: 'Weekly',       expr: '0 0 * * 1' },
  { id: 'custom',       label: 'Custom',       expr: '' },
];

function exprToPreset(expr: string): CronPreset {
  const match = CRON_PRESETS.find((p) => p.id !== 'custom' && p.expr === expr);
  return match ? match.id : 'custom';
}

/** Convert an ISO 8601 string to the "YYYY-MM-DDTHH:MM" format required by datetime-local inputs. */
function toDatetimeLocal(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return '';
  const pad = (n: number) => n.toString().padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

const SCHEDULE_TYPES: { type: Schedule['type']; label: string }[] = [
  { type: 'Cron',    label: 'Recurring' },
  { type: 'OneShot', label: 'One time' },
  { type: 'OnLogin', label: 'On login' },
  { type: 'OnWake',  label: 'On wake' },
];

export default function ScheduleBuilder({ value, onChange }: ScheduleBuilderProps) {
  function handleTypeChange(type: Schedule['type']) {
    switch (type) {
      case 'Cron':
        onChange({ type: 'Cron', expression: '0 * * * *' });
        break;
      case 'OneShot': {
        // default to now + 1 hour, stored as ISO 8601
        onChange({ type: 'OneShot', run_at: new Date(Date.now() + 3_600_000).toISOString() });
        break;
      }
      case 'OnLogin':
        onChange({ type: 'OnLogin' });
        break;
      case 'OnWake':
        onChange({ type: 'OnWake' });
        break;
    }
  }

  function handlePresetChange(preset: CronPreset) {
    if (value.type !== 'Cron') return;
    if (preset === 'custom') {
      onChange({ type: 'Cron', expression: value.expression });
    } else {
      const p = CRON_PRESETS.find((p) => p.id === preset)!;
      onChange({ type: 'Cron', expression: p.expr });
    }
  }

  const activePreset = value.type === 'Cron' ? exprToPreset(value.expression) : null;

  return (
    <div className="flex flex-col gap-3">
      {/* Type selector */}
      <div className="flex gap-1 rounded-lg bg-muted p-0.5">
        {SCHEDULE_TYPES.map(({ type, label }) => (
          <button
            key={type}
            type="button"
            onClick={() => handleTypeChange(type)}
            className={[
              'flex-1 rounded-md px-1.5 py-1 text-[11px] font-medium transition-colors',
              value.type === type
                ? 'bg-background text-foreground shadow-sm'
                : 'text-muted-foreground hover:text-foreground',
            ].join(' ')}
          >
            {label}
          </button>
        ))}
      </div>

      {/* Cron — frequency presets */}
      {value.type === 'Cron' && (
        <div className="flex flex-col gap-2">
          <Label className="text-xs text-muted-foreground">Frequency</Label>
          <div className="grid grid-cols-3 gap-1">
            {CRON_PRESETS.map((preset) => (
              <button
                key={preset.id}
                type="button"
                onClick={() => handlePresetChange(preset.id)}
                className={[
                  'rounded-md border px-2 py-1 text-[11px] transition-colors',
                  activePreset === preset.id
                    ? 'border-ring bg-accent text-accent-foreground'
                    : 'border-border text-muted-foreground hover:border-ring/50 hover:text-foreground',
                ].join(' ')}
              >
                {preset.label}
              </button>
            ))}
          </div>
          {activePreset === 'custom' && (
            <div className="flex flex-col gap-1">
              <Label htmlFor="cron-expr" className="text-xs text-muted-foreground">
                Cron expression
              </Label>
              <Input
                id="cron-expr"
                value={value.expression}
                onChange={(e) => onChange({ type: 'Cron', expression: e.target.value })}
                placeholder="* * * * *"
                className="h-7 font-mono text-xs"
              />
            </div>
          )}
        </div>
      )}

      {/* OneShot — datetime picker */}
      {value.type === 'OneShot' && (
        <div className="flex flex-col gap-1">
          <Label htmlFor="run-at" className="text-xs text-muted-foreground">
            Run at
          </Label>
          <Input
            id="run-at"
            type="datetime-local"
            value={toDatetimeLocal(value.run_at)}
            onChange={(e) => onChange({ type: 'OneShot', run_at: new Date(e.target.value).toISOString() })}
            className="h-7 text-xs"
          />
        </div>
      )}

      {/* OnLogin / OnWake — no extra fields */}
      {(value.type === 'OnLogin' || value.type === 'OnWake') && (
        <p className="text-xs text-muted-foreground">
          {value.type === 'OnLogin'
            ? 'Task will run each time you log in.'
            : 'Task will run each time the system wakes from sleep.'}
        </p>
      )}
    </div>
  );
}
