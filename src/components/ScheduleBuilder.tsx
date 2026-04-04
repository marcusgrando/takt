// src/components/ScheduleBuilder.tsx
import { useState, useCallback } from 'react';
import { type Schedule } from '@/lib/api';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import TimeField from './schedule/TimeField';
import DayGrid from './schedule/DayGrid';
import WeekdayGrid from './schedule/WeekdayGrid';
import {
  type RecurringState,
  type FrequencyType,
  type IntervalUnit,
  type OrdinalPosition,
  DEFAULT_RECURRING,
  buildCron,
  parseCron,
} from './schedule/cron-utils';

interface ScheduleBuilderProps {
  value: Schedule;
  onChange: (s: Schedule) => void;
}

const SCHEDULE_TYPES: { type: Schedule['type']; label: string }[] = [
  { type: 'Cron', label: 'Recurring' },
  { type: 'OneShot', label: 'One time' },
  { type: 'DailyFirstUse', label: 'Daily first use' },
];

const FREQUENCIES: { value: FrequencyType; label: string }[] = [
  { value: 'hourly', label: 'Hourly' },
  { value: 'daily', label: 'Daily' },
  { value: 'weekly', label: 'Weekly' },
  { value: 'monthly', label: 'Monthly' },
  { value: 'custom', label: 'Custom' },
];

const ORDINAL_OPTIONS: { value: OrdinalPosition; label: string }[] = [
  { value: 'first', label: 'First' },
  { value: 'second', label: 'Second' },
  { value: 'third', label: 'Third' },
  { value: 'fourth', label: 'Fourth' },
  { value: 'last', label: 'Last' },
];

const WEEKDAY_OPTIONS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];

export default function ScheduleBuilder({ value, onChange }: ScheduleBuilderProps) {
  // Recurring state — initialized by parsing the current cron expression
  const [recurring, setRecurring] = useState<RecurringState>(() =>
    value.type === 'Cron' ? parseCron(value.expression) : DEFAULT_RECURRING,
  );

  // OneShot state — split into date and time parts
  const [oneShotDate, setOneShotDate] = useState(() => {
    if (value.type !== 'OneShot') return '';
    const d = new Date(value.run_at);
    if (isNaN(d.getTime())) return '';
    return `${d.getFullYear()}-${(d.getMonth() + 1).toString().padStart(2, '0')}-${d.getDate().toString().padStart(2, '0')}`;
  });
  const [oneShotHour, setOneShotHour] = useState(() => {
    if (value.type !== 'OneShot') return 9;
    const d = new Date(value.run_at);
    return isNaN(d.getTime()) ? 9 : d.getHours();
  });
  const [oneShotMinute, setOneShotMinute] = useState(() => {
    if (value.type !== 'OneShot') return 0;
    const d = new Date(value.run_at);
    return isNaN(d.getTime()) ? 0 : d.getMinutes();
  });

  // Update recurring and emit cron
  const updateRecurring = useCallback(
    (patch: Partial<RecurringState>) => {
      setRecurring((prev) => {
        const next = { ...prev, ...patch };
        onChange({ type: 'Cron', expression: buildCron(next) });
        return next;
      });
    },
    [onChange],
  );

  // Emit OneShot
  function emitOneShot(date: string, hour: number, minute: number) {
    if (!date) return;
    const d = new Date(`${date}T${hour.toString().padStart(2, '0')}:${minute.toString().padStart(2, '0')}:00`);
    if (!isNaN(d.getTime())) {
      onChange({ type: 'OneShot', run_at: d.toISOString() });
    }
  }

  function handleTypeChange(v: string) {
    const type = v as Schedule['type'];
    switch (type) {
      case 'Cron':
        onChange({ type: 'Cron', expression: buildCron(recurring) });
        break;
      case 'OneShot': {
        const date = oneShotDate || new Date(Date.now() + 3_600_000).toISOString().slice(0, 10);
        if (!oneShotDate) setOneShotDate(date);
        emitOneShot(date, oneShotHour, oneShotMinute);
        break;
      }
      case 'DailyFirstUse':
        onChange({ type: 'DailyFirstUse', delay_minutes: 5 });
        break;
    }
  }

  return (
    <div className="space-y-4">
      {/* Schedule type tabs */}
      <Tabs value={value.type} onValueChange={handleTypeChange}>
        <TabsList className="w-full">
          {SCHEDULE_TYPES.map(({ type, label }) => (
            <TabsTrigger key={type} value={type}>{label}</TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      {/* ── Recurring ── */}
      {value.type === 'Cron' && (
        <div className="space-y-4">
          {/* Frequency selector */}
          <div className="flex items-center justify-between">
            <Label className="text-sm font-medium">Frequency</Label>
            <Select
              value={recurring.frequency}
              onValueChange={(v) => updateRecurring({ frequency: v as FrequencyType })}
            >
              <SelectTrigger className="w-auto">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {FREQUENCIES.map(({ value, label }) => (
                  <SelectItem key={value} value={value}>{label}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {/* ── Hourly ── */}
          {recurring.frequency === 'hourly' && (
            <div className="flex items-center gap-2">
              <span className="text-sm text-muted-foreground">Every</span>
              <Input
                type="number"
                min={1}
                max={recurring.intervalUnit === 'minutes' ? 59 : 23}
                value={recurring.intervalValue}
                onChange={(e) => {
                  const v = parseInt(e.target.value);
                  if (!isNaN(v) && v >= 1) updateRecurring({ intervalValue: v });
                }}
                className="w-16 text-center [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
              />
              <Select
                value={recurring.intervalUnit}
                onValueChange={(v) => updateRecurring({ intervalUnit: v as IntervalUnit })}
              >
                <SelectTrigger className="w-auto">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="minutes">Minutes</SelectItem>
                  <SelectItem value="hours">Hours</SelectItem>
                </SelectContent>
              </Select>
            </div>
          )}

          {/* ── Daily ── */}
          {recurring.frequency === 'daily' && (
            <TimeField
              hour={recurring.hour}
              minute={recurring.minute}
              onChange={(h, m) => updateRecurring({ hour: h, minute: m })}
            />
          )}

          {/* ── Weekly ── */}
          {recurring.frequency === 'weekly' && (
            <div className="space-y-3">
              <WeekdayGrid
                selected={recurring.weekdays}
                onChange={(days) => updateRecurring({ weekdays: days })}
              />
              <Separator />
              <TimeField
                hour={recurring.hour}
                minute={recurring.minute}
                onChange={(h, m) => updateRecurring({ hour: h, minute: m })}
              />
            </div>
          )}

          {/* ── Monthly ── */}
          {recurring.frequency === 'monthly' && (
            <div className="space-y-3">
              {/* Each / On the radio */}
              <div className="space-y-3">
                <label className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="radio"
                    name="monthly-mode"
                    checked={recurring.monthlyMode === 'each'}
                    onChange={() => updateRecurring({ monthlyMode: 'each' })}
                    className="accent-primary"
                  />
                  <span className="text-sm font-medium">Each</span>
                </label>

                {recurring.monthlyMode === 'each' && (
                  <DayGrid
                    selected={recurring.monthDays}
                    onChange={(days) => updateRecurring({ monthDays: days })}
                  />
                )}

                <label className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="radio"
                    name="monthly-mode"
                    checked={recurring.monthlyMode === 'onThe'}
                    onChange={() => updateRecurring({ monthlyMode: 'onThe' })}
                    className="accent-primary"
                  />
                  <span className="text-sm font-medium">On the</span>
                </label>

                {recurring.monthlyMode === 'onThe' && (
                  <div className="flex gap-2">
                    <Select
                      value={recurring.ordinalPosition}
                      onValueChange={(v) => updateRecurring({ ordinalPosition: v as OrdinalPosition })}
                    >
                      <SelectTrigger className="w-auto">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {ORDINAL_OPTIONS.map(({ value, label }) => (
                          <SelectItem key={value} value={value}>{label}</SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <Select
                      value={recurring.ordinalWeekday.toString()}
                      onValueChange={(v) => updateRecurring({ ordinalWeekday: parseInt(v) })}
                    >
                      <SelectTrigger className="w-auto">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {WEEKDAY_OPTIONS.map((day, i) => (
                          <SelectItem key={i} value={i.toString()}>{day}</SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                )}
              </div>

              <Separator />
              <TimeField
                hour={recurring.hour}
                minute={recurring.minute}
                onChange={(h, m) => updateRecurring({ hour: h, minute: m })}
              />
            </div>
          )}

          {/* ── Custom ── */}
          {recurring.frequency === 'custom' && (
            <div className="space-y-2">
              <Label htmlFor="cron-expr">Cron expression</Label>
              <Input
                id="cron-expr"
                value={recurring.customExpression}
                onChange={(e) => {
                  const expr = e.target.value;
                  setRecurring((prev) => ({ ...prev, customExpression: expr }));
                  onChange({ type: 'Cron', expression: expr });
                }}
                placeholder="0 * * * *"
                className="font-mono"
              />
            </div>
          )}
        </div>
      )}

      {/* ── One Time ── */}
      {value.type === 'OneShot' && (
        <div className="space-y-3">
          <div className="space-y-1.5">
            <Label htmlFor="oneshot-date" className="text-sm font-medium">Date</Label>
            <Input
              id="oneshot-date"
              type="date"
              value={oneShotDate}
              onChange={(e) => {
                setOneShotDate(e.target.value);
                emitOneShot(e.target.value, oneShotHour, oneShotMinute);
              }}
            />
          </div>
          <TimeField
            hour={oneShotHour}
            minute={oneShotMinute}
            onChange={(h, m) => {
              setOneShotHour(h);
              setOneShotMinute(m);
              emitOneShot(oneShotDate, h, m);
            }}
          />
        </div>
      )}

      {/* ── Daily First Use ── */}
      {value.type === 'DailyFirstUse' && (
        <div className="space-y-3">
          <p className="text-sm text-muted-foreground">
            Runs once per day after continuous active use of your Mac.
          </p>
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">After</span>
            <Input
              type="number"
              min={1}
              max={60}
              value={value.delay_minutes}
              onChange={(e) => {
                const v = parseInt(e.target.value);
                if (!isNaN(v) && v >= 1) onChange({ type: 'DailyFirstUse', delay_minutes: v });
              }}
              className="w-16 text-center [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
            />
            <span className="text-sm text-muted-foreground">{value.delay_minutes === 1 ? 'minute' : 'minutes'}</span>
          </div>
        </div>
      )}
    </div>
  );
}
