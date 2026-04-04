import { type Schedule } from '@/lib/api';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';

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
  return CRON_PRESETS.find((p) => p.id !== 'custom' && p.expr === expr)?.id ?? 'custom';
}

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
  function handleTypeChange(v: string) {
    const type = v as Schedule['type'];
    switch (type) {
      case 'Cron': onChange({ type: 'Cron', expression: '0 * * * *' }); break;
      case 'OneShot': onChange({ type: 'OneShot', run_at: new Date(Date.now() + 3_600_000).toISOString() }); break;
      case 'OnLogin': onChange({ type: 'OnLogin' }); break;
      case 'OnWake': onChange({ type: 'OnWake' }); break;
    }
  }

  function handlePresetChange(preset: CronPreset) {
    if (value.type !== 'Cron') return;
    onChange({ type: 'Cron', expression: preset === 'custom' ? '' : CRON_PRESETS.find((p) => p.id === preset)!.expr });
  }

  const activePreset = value.type === 'Cron' ? exprToPreset(value.expression) : null;

  return (
    <div className="space-y-4">
      {/* Schedule type — shadcn Tabs */}
      <Tabs value={value.type} onValueChange={handleTypeChange}>
        <TabsList className="w-full">
          {SCHEDULE_TYPES.map(({ type, label }) => (
            <TabsTrigger key={type} value={type}>{label}</TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      {value.type === 'Cron' && (
        <div className="space-y-3">
          <Label>Frequency</Label>
          <div className="grid grid-cols-3 gap-2">
            {CRON_PRESETS.map((preset) => (
              <Button
                key={preset.id}
                type="button"
                variant={activePreset === preset.id ? 'default' : 'outline'}
                size="sm"
                onClick={() => handlePresetChange(preset.id)}
                className="w-full"
              >
                {preset.label}
              </Button>
            ))}
          </div>
          {activePreset === 'custom' && (
            <div className="space-y-2">
              <Label htmlFor="cron-expr">Cron expression</Label>
              <Input id="cron-expr" value={value.expression} onChange={(e) => onChange({ type: 'Cron', expression: e.target.value })} placeholder="0 * * * *" className="font-mono" />
            </div>
          )}
        </div>
      )}

      {value.type === 'OneShot' && (
        <div className="space-y-2">
          <Label htmlFor="run-at">Run at</Label>
          <Input id="run-at" type="datetime-local" value={toDatetimeLocal(value.run_at)} onChange={(e) => onChange({ type: 'OneShot', run_at: e.target.value ? new Date(e.target.value).toISOString() : '' })} />
        </div>
      )}

      {(value.type === 'OnLogin' || value.type === 'OnWake') && (
        <p className="text-sm text-muted-foreground">
          {value.type === 'OnLogin' ? 'Task will run each time you log in.' : 'Task will run each time the system wakes from sleep.'}
        </p>
      )}
    </div>
  );
}
