# ScheduleBuilder Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign ScheduleBuilder from cron-preset buttons to a frequency-based UI inspired by macOS Reminders, keeping the Schedule enum unchanged.

**Architecture:** The frontend ScheduleBuilder component is rewritten with sub-components for each frequency mode. A cron parser/builder module handles bidirectional conversion between structured schedule state and cron expressions. No backend changes needed.

**Tech Stack:** React, TypeScript, shadcn/ui (Select, Button, Input, Tabs), Tailwind CSS

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/components/schedule/cron-utils.ts` | Cron expression ↔ structured state conversion (parse + build) |
| `src/components/schedule/TimeField.tsx` | Reusable HH:MM input pair |
| `src/components/schedule/DayGrid.tsx` | Monthly day grid (1-31) with multi-select toggle |
| `src/components/schedule/WeekdayGrid.tsx` | Weekly day-of-week grid (Sun-Sat) with multi-select toggle |
| `src/components/ScheduleBuilder.tsx` | Main component rewrite — frequency selector + sub-components |
| `src/components/AutoName.ts` | Update `describeSchedule` to produce better names from new frequency patterns |

---

### Task 1: Cron Parser/Builder Utility

**Files:**
- Create: `src/components/schedule/cron-utils.ts`

This module defines the structured schedule state and provides two functions: `parseCron` (cron string → structured state) and `buildCron` (structured state → cron string).

- [ ] **Step 1: Create the structured types and buildCron function**

```ts
// src/components/schedule/cron-utils.ts

export type FrequencyType = 'hourly' | 'daily' | 'weekly' | 'monthly' | 'custom';
export type IntervalUnit = 'minutes' | 'hours';
export type MonthlyMode = 'each' | 'onThe';
export type OrdinalPosition = 'first' | 'second' | 'third' | 'fourth' | 'last';

export interface RecurringState {
  frequency: FrequencyType;
  // Hourly
  intervalValue: number;
  intervalUnit: IntervalUnit;
  // Daily
  dailyInterval: number;
  // Weekly
  weeklyInterval: number;
  weekdays: number[]; // 0=Sun, 1=Mon, ..., 6=Sat
  // Monthly
  monthlyInterval: number;
  monthlyMode: MonthlyMode;
  monthDays: number[]; // 1-31
  ordinalPosition: OrdinalPosition;
  ordinalWeekday: number; // 0=Sun, ..., 6=Sat
  // Time (used by daily, weekly, monthly)
  hour: number;
  minute: number;
  // Custom
  customExpression: string;
}

export const DEFAULT_RECURRING: RecurringState = {
  frequency: 'daily',
  intervalValue: 1,
  intervalUnit: 'hours',
  dailyInterval: 1,
  weeklyInterval: 1,
  weekdays: [1], // Monday
  monthlyInterval: 1,
  monthlyMode: 'each',
  monthDays: [1],
  ordinalPosition: 'first',
  ordinalWeekday: 1, // Monday
  hour: 9,
  minute: 0,
  customExpression: '0 * * * *',
};

const ORDINAL_MAP: Record<OrdinalPosition, string> = {
  first: '1',
  second: '2',
  third: '3',
  fourth: '4',
  last: 'L',
};

export function buildCron(state: RecurringState): string {
  switch (state.frequency) {
    case 'hourly': {
      if (state.intervalUnit === 'minutes') {
        return state.intervalValue === 1
          ? '* * * * *'
          : `*/${state.intervalValue} * * * *`;
      }
      // hours
      return state.intervalValue === 1
        ? '0 * * * *'
        : `0 */${state.intervalValue} * * *`;
    }
    case 'daily': {
      const dayPart = state.dailyInterval > 1 ? `*/${state.dailyInterval}` : '*';
      return `${state.minute} ${state.hour} ${dayPart} * *`;
    }
    case 'weekly': {
      const days = state.weekdays.length > 0 ? state.weekdays.sort((a, b) => a - b).join(',') : '1';
      return `${state.minute} ${state.hour} * * ${days}`;
    }
    case 'monthly': {
      const monthPart = state.monthlyInterval > 1 ? `*/${state.monthlyInterval}` : '*';
      if (state.monthlyMode === 'each') {
        const days = state.monthDays.length > 0 ? state.monthDays.sort((a, b) => a - b).join(',') : '1';
        return `${state.minute} ${state.hour} ${days} ${monthPart} *`;
      }
      // "On the" mode: use weekday#ordinal syntax
      const ord = ORDINAL_MAP[state.ordinalPosition];
      if (ord === 'L') {
        return `${state.minute} ${state.hour} * ${monthPart} ${state.ordinalWeekday}L`;
      }
      return `${state.minute} ${state.hour} * ${monthPart} ${state.ordinalWeekday}#${ord}`;
    }
    case 'custom':
      return state.customExpression;
  }
}
```

- [ ] **Step 2: Add the parseCron function**

Add this to the same file, after `buildCron`:

```ts
export function parseCron(expression: string): RecurringState {
  const parts = expression.trim().split(/\s+/);
  if (parts.length !== 5) {
    return { ...DEFAULT_RECURRING, frequency: 'custom', customExpression: expression };
  }

  const [minField, hourField, domField, monField, dowField] = parts;

  // Try to detect hourly: minute or hour field has */N or *, and dom/mon/dow are all *
  if (domField === '*' && monField === '*' && dowField === '*') {
    // Hourly with minutes interval: */N * * * * or * * * * *
    if (hourField === '*') {
      const minInterval = parseInterval(minField);
      if (minInterval !== null) {
        return {
          ...DEFAULT_RECURRING,
          frequency: 'hourly',
          intervalUnit: 'minutes',
          intervalValue: minInterval,
        };
      }
    }

    // Hourly with hours interval: 0 */N * * * or N N * * *
    if (minField.match(/^\d+$/) && hourField.match(/^\*\/?\d*$/)) {
      const hourInterval = parseInterval(hourField);
      if (hourInterval !== null && hourInterval > 1) {
        return {
          ...DEFAULT_RECURRING,
          frequency: 'hourly',
          intervalUnit: 'hours',
          intervalValue: hourInterval,
        };
      }
      if (hourInterval === 1) {
        return {
          ...DEFAULT_RECURRING,
          frequency: 'hourly',
          intervalUnit: 'hours',
          intervalValue: 1,
        };
      }
    }

    // Daily: M H */N * * or M H * * *
    if (minField.match(/^\d+$/) && hourField.match(/^\d+$/)) {
      const dailyInterval = parseInterval(domField);
      if (dailyInterval !== null) {
        return {
          ...DEFAULT_RECURRING,
          frequency: 'daily',
          hour: parseInt(hourField),
          minute: parseInt(minField),
          dailyInterval,
        };
      }
    }
  }

  // Weekly: M H * * 0,1,3 (dow is not * and dom is *)
  if (domField === '*' && dowField !== '*' && !dowField.includes('#') && !dowField.includes('L')) {
    const min = parseInt(minField);
    const hour = parseInt(hourField);
    if (!isNaN(min) && !isNaN(hour) && monField === '*') {
      const weekdays = dowField.split(',').map(Number).filter((n) => !isNaN(n));
      if (weekdays.length > 0) {
        return {
          ...DEFAULT_RECURRING,
          frequency: 'weekly',
          hour,
          minute: min,
          weekdays,
        };
      }
    }
  }

  // Monthly "each": M H 1,15 * * or M H 1,15 */N *
  if (domField !== '*' && !domField.includes('/') && (dowField === '*')) {
    const min = parseInt(minField);
    const hour = parseInt(hourField);
    if (!isNaN(min) && !isNaN(hour)) {
      const monthDays = domField.split(',').map(Number).filter((n) => !isNaN(n) && n >= 1 && n <= 31);
      if (monthDays.length > 0) {
        const monthlyInterval = parseInterval(monField) ?? 1;
        return {
          ...DEFAULT_RECURRING,
          frequency: 'monthly',
          hour,
          minute: min,
          monthlyMode: 'each',
          monthDays,
          monthlyInterval,
        };
      }
    }
  }

  // Monthly "on the": M H * * 1#2 or M H * */N 1L
  if (domField === '*' && dowField !== '*' && (dowField.includes('#') || dowField.includes('L'))) {
    const min = parseInt(minField);
    const hour = parseInt(hourField);
    if (!isNaN(min) && !isNaN(hour)) {
      const monthlyInterval = parseInterval(monField) ?? 1;
      if (dowField.includes('#')) {
        const [dayStr, ordStr] = dowField.split('#');
        const weekday = parseInt(dayStr);
        const ordNum = parseInt(ordStr);
        const posMap: Record<number, OrdinalPosition> = { 1: 'first', 2: 'second', 3: 'third', 4: 'fourth' };
        if (!isNaN(weekday) && posMap[ordNum]) {
          return {
            ...DEFAULT_RECURRING,
            frequency: 'monthly',
            hour,
            minute: min,
            monthlyMode: 'onThe',
            ordinalPosition: posMap[ordNum],
            ordinalWeekday: weekday,
            monthlyInterval,
          };
        }
      } else if (dowField.endsWith('L')) {
        const weekday = parseInt(dowField.replace('L', ''));
        if (!isNaN(weekday)) {
          return {
            ...DEFAULT_RECURRING,
            frequency: 'monthly',
            hour,
            minute: min,
            monthlyMode: 'onThe',
            ordinalPosition: 'last',
            ordinalWeekday: weekday,
            monthlyInterval,
          };
        }
      }
    }
  }

  // Fallback: custom
  return { ...DEFAULT_RECURRING, frequency: 'custom', customExpression: expression };
}

function parseInterval(field: string): number | null {
  if (field === '*') return 1;
  const m = field.match(/^\*\/(\d+)$/);
  if (m) return parseInt(m[1]);
  if (field.match(/^\d+$/)) return parseInt(field);
  return null;
}
```

- [ ] **Step 3: Verify the module compiles**

Run: `cd /Users/marcus.grando/git/cronmac && npx tsc --noEmit --pretty 2>&1 | head -20`
Expected: No errors related to `cron-utils.ts`

- [ ] **Step 4: Commit**

```bash
git add src/components/schedule/cron-utils.ts
git commit -m "$(cat <<'EOF'
feat: add cron parser/builder utility for schedule redesign
EOF
)"
```

---

### Task 2: TimeField Component

**Files:**
- Create: `src/components/schedule/TimeField.tsx`

Reusable HH:MM input pair used by Daily, Weekly, Monthly, and OneShot.

- [ ] **Step 1: Create TimeField component**

```tsx
// src/components/schedule/TimeField.tsx
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

interface TimeFieldProps {
  hour: number;
  minute: number;
  onChange: (hour: number, minute: number) => void;
  label?: string;
}

export default function TimeField({ hour, minute, onChange, label = 'Time' }: TimeFieldProps) {
  function handleHourChange(value: string) {
    const h = parseInt(value);
    if (!isNaN(h) && h >= 0 && h <= 23) onChange(h, minute);
  }

  function handleMinuteChange(value: string) {
    const m = parseInt(value);
    if (!isNaN(m) && m >= 0 && m <= 59) onChange(hour, m);
  }

  return (
    <div className="flex items-center justify-between">
      <Label className="text-sm font-medium">{label}</Label>
      <div className="flex items-center gap-1">
        <Input
          type="number"
          min={0}
          max={23}
          value={hour.toString().padStart(2, '0')}
          onChange={(e) => handleHourChange(e.target.value)}
          className="w-12 text-center font-medium tabular-nums px-1 [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
        />
        <span className="text-sm font-medium">:</span>
        <Input
          type="number"
          min={0}
          max={59}
          value={minute.toString().padStart(2, '0')}
          onChange={(e) => handleMinuteChange(e.target.value)}
          className="w-12 text-center font-medium tabular-nums px-1 [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
        />
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /Users/marcus.grando/git/cronmac && npx tsc --noEmit --pretty 2>&1 | head -20`
Expected: No errors related to `TimeField.tsx`

- [ ] **Step 3: Commit**

```bash
git add src/components/schedule/TimeField.tsx
git commit -m "$(cat <<'EOF'
feat: add TimeField component for HH:MM input
EOF
)"
```

---

### Task 3: DayGrid Component

**Files:**
- Create: `src/components/schedule/DayGrid.tsx`

Grid of days 1-31 with multi-select toggle for monthly scheduling.

- [ ] **Step 1: Create DayGrid component**

```tsx
// src/components/schedule/DayGrid.tsx
import { Button } from '@/components/ui/button';

interface DayGridProps {
  selected: number[];
  onChange: (days: number[]) => void;
}

export default function DayGrid({ selected, onChange }: DayGridProps) {
  function toggle(day: number) {
    if (selected.includes(day)) {
      const next = selected.filter((d) => d !== day);
      if (next.length > 0) onChange(next); // keep at least 1
    } else {
      onChange([...selected, day]);
    }
  }

  return (
    <div className="grid grid-cols-7 gap-1">
      {Array.from({ length: 31 }, (_, i) => i + 1).map((day) => (
        <Button
          key={day}
          type="button"
          variant={selected.includes(day) ? 'default' : 'outline'}
          size="sm"
          onClick={() => toggle(day)}
          className="h-7 w-7 p-0 text-xs font-medium"
        >
          {day}
        </Button>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /Users/marcus.grando/git/cronmac && npx tsc --noEmit --pretty 2>&1 | head -20`
Expected: No errors related to `DayGrid.tsx`

- [ ] **Step 3: Commit**

```bash
git add src/components/schedule/DayGrid.tsx
git commit -m "$(cat <<'EOF'
feat: add DayGrid component for monthly day selection
EOF
)"
```

---

### Task 4: WeekdayGrid Component

**Files:**
- Create: `src/components/schedule/WeekdayGrid.tsx`

Horizontal row of 7 weekday buttons (Sun-Sat) with multi-select toggle.

- [ ] **Step 1: Create WeekdayGrid component**

```tsx
// src/components/schedule/WeekdayGrid.tsx
import { Button } from '@/components/ui/button';

const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'] as const;

interface WeekdayGridProps {
  selected: number[]; // 0=Sun, ..., 6=Sat
  onChange: (days: number[]) => void;
}

export default function WeekdayGrid({ selected, onChange }: WeekdayGridProps) {
  function toggle(day: number) {
    if (selected.includes(day)) {
      const next = selected.filter((d) => d !== day);
      if (next.length > 0) onChange(next); // keep at least 1
    } else {
      onChange([...selected, day]);
    }
  }

  return (
    <div className="flex gap-1">
      {DAYS.map((label, i) => (
        <Button
          key={i}
          type="button"
          variant={selected.includes(i) ? 'default' : 'outline'}
          size="sm"
          onClick={() => toggle(i)}
          className="h-8 flex-1 p-0 text-xs font-medium"
        >
          {label}
        </Button>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /Users/marcus.grando/git/cronmac && npx tsc --noEmit --pretty 2>&1 | head -20`
Expected: No errors related to `WeekdayGrid.tsx`

- [ ] **Step 3: Commit**

```bash
git add src/components/schedule/WeekdayGrid.tsx
git commit -m "$(cat <<'EOF'
feat: add WeekdayGrid component for weekly day selection
EOF
)"
```

---

### Task 5: Rewrite ScheduleBuilder

**Files:**
- Modify: `src/components/ScheduleBuilder.tsx` (complete rewrite)

This is the main task. Rewrites the component to use frequency-based UI with all sub-components.

- [ ] **Step 1: Rewrite ScheduleBuilder.tsx**

```tsx
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
  type MonthlyMode,
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
        onChange({ type: 'DailyFirstUse' });
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
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <span className="text-sm text-muted-foreground">Every</span>
                <Input
                  type="number"
                  min={1}
                  max={365}
                  value={recurring.dailyInterval}
                  onChange={(e) => {
                    const v = parseInt(e.target.value);
                    if (!isNaN(v) && v >= 1) updateRecurring({ dailyInterval: v });
                  }}
                  className="w-16 text-center [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
                />
                <span className="text-sm text-muted-foreground">{recurring.dailyInterval === 1 ? 'Day' : 'Days'}</span>
              </div>
              <Separator />
              <TimeField
                hour={recurring.hour}
                minute={recurring.minute}
                onChange={(h, m) => updateRecurring({ hour: h, minute: m })}
              />
            </div>
          )}

          {/* ── Weekly ── */}
          {recurring.frequency === 'weekly' && (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <span className="text-sm text-muted-foreground">Every</span>
                <Input
                  type="number"
                  min={1}
                  max={52}
                  value={recurring.weeklyInterval}
                  onChange={(e) => {
                    const v = parseInt(e.target.value);
                    if (!isNaN(v) && v >= 1) updateRecurring({ weeklyInterval: v });
                  }}
                  className="w-16 text-center [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
                />
                <span className="text-sm text-muted-foreground">{recurring.weeklyInterval === 1 ? 'Week' : 'Weeks'}</span>
              </div>
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
              <div className="flex items-center gap-2">
                <span className="text-sm text-muted-foreground">Every</span>
                <Input
                  type="number"
                  min={1}
                  max={12}
                  value={recurring.monthlyInterval}
                  onChange={(e) => {
                    const v = parseInt(e.target.value);
                    if (!isNaN(v) && v >= 1) updateRecurring({ monthlyInterval: v });
                  }}
                  className="w-16 text-center [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
                />
                <span className="text-sm text-muted-foreground">{recurring.monthlyInterval === 1 ? 'Month' : 'Months'}</span>
              </div>

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
        <p className="text-sm text-muted-foreground">
          Runs once per day, 5 minutes after you start using your Mac.
        </p>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /Users/marcus.grando/git/cronmac && npx tsc --noEmit --pretty 2>&1 | head -20`
Expected: No errors

- [ ] **Step 3: Manual test — open the app in dev mode**

Run: `cd /Users/marcus.grando/git/cronmac && bun run dev` (in one terminal)
Then in another: `cd /Users/marcus.grando/git/cronmac/src-tauri && cargo tauri dev`

Test:
1. Click + to create a new task
2. Verify Recurring tab shows frequency dropdown with Hourly/Daily/Weekly/Monthly/Custom
3. Switch frequencies and verify each shows correct controls
4. Verify time field updates correctly
5. Switch to One Time and verify date + time fields
6. Switch to Daily first use and verify text
7. Edit an existing task and verify the cron expression is parsed back correctly

- [ ] **Step 4: Commit**

```bash
git add src/components/ScheduleBuilder.tsx
git commit -m "$(cat <<'EOF'
feat: rewrite ScheduleBuilder with Reminders-style frequency UI
EOF
)"
```

---

### Task 6: Update AutoName for New Frequencies

**Files:**
- Modify: `src/components/AutoName.ts`

Update `describeSchedule` to produce better human-readable names from the new frequency-based cron patterns.

- [ ] **Step 1: Rewrite describeSchedule in AutoName.ts**

Replace the `describeSchedule` function in `src/components/AutoName.ts` with:

```ts
import { parseCron } from './schedule/cron-utils';

function describeSchedule(schedule: Schedule): string {
  switch (schedule.type) {
    case 'DailyFirstUse': return 'Daily first use';
    case 'OneShot': {
      const d = new Date(schedule.run_at);
      if (isNaN(d.getTime())) return 'One time';
      return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
    }
    case 'Cron': {
      const state = parseCron(schedule.expression);
      switch (state.frequency) {
        case 'hourly':
          return state.intervalUnit === 'minutes'
            ? `Every ${state.intervalValue} min`
            : `Every ${state.intervalValue} hour${state.intervalValue > 1 ? 's' : ''}`;
        case 'daily':
          return state.dailyInterval === 1
            ? `Daily at ${formatTime(state.hour, state.minute)}`
            : `Every ${state.dailyInterval} days at ${formatTime(state.hour, state.minute)}`;
        case 'weekly': {
          const dayNames = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
          const days = state.weekdays.sort((a, b) => a - b).map((d) => dayNames[d]).join(', ');
          return `${days} at ${formatTime(state.hour, state.minute)}`;
        }
        case 'monthly': {
          if (state.monthlyMode === 'each') {
            const days = state.monthDays.sort((a, b) => a - b).join(', ');
            return `Monthly on ${days} at ${formatTime(state.hour, state.minute)}`;
          }
          return `Monthly ${state.ordinalPosition} ${['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'][state.ordinalWeekday]} at ${formatTime(state.hour, state.minute)}`;
        }
        case 'custom':
          return `Cron ${schedule.expression}`;
      }
    }
  }
}

function formatTime(hour: number, minute: number): string {
  return `${hour.toString().padStart(2, '0')}:${minute.toString().padStart(2, '0')}`;
}
```

The full file should look like:

```ts
import type { Action, Schedule } from '@/lib/api';
import { parseCron } from './schedule/cron-utils';

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
    case 'OpenApp': {
      if (!action.app_path) return 'Open app';
      const name = action.app_path.split('/').pop()?.replace('.app', '') || action.app_path;
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
    case 'Webhook': {
      if (!action.url) return 'Webhook';
      try {
        const host = new URL(action.url).hostname.replace(/^www\./, '');
        return `${action.method} ${host}`;
      } catch {
        return `${action.method} webhook`;
      }
    }
    case 'Settings': {
      const id = action.pane_url.split(':')[1] ?? '';
      const name = id.split('.').pop()?.replace('-Settings', '').replace('.extension', '').replace('-', ' ') || 'Settings';
      return `Open ${name}`;
    }
  }
}

function formatTime(hour: number, minute: number): string {
  return `${hour.toString().padStart(2, '0')}:${minute.toString().padStart(2, '0')}`;
}

function describeSchedule(schedule: Schedule): string {
  switch (schedule.type) {
    case 'DailyFirstUse': return 'Daily first use';
    case 'OneShot': {
      const d = new Date(schedule.run_at);
      if (isNaN(d.getTime())) return 'One time';
      return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
    }
    case 'Cron': {
      const state = parseCron(schedule.expression);
      switch (state.frequency) {
        case 'hourly':
          return state.intervalUnit === 'minutes'
            ? `Every ${state.intervalValue} min`
            : `Every ${state.intervalValue} hour${state.intervalValue > 1 ? 's' : ''}`;
        case 'daily':
          return state.dailyInterval === 1
            ? `Daily at ${formatTime(state.hour, state.minute)}`
            : `Every ${state.dailyInterval} days at ${formatTime(state.hour, state.minute)}`;
        case 'weekly': {
          const dayNames = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
          const days = state.weekdays.sort((a, b) => a - b).map((d) => dayNames[d]).join(', ');
          return `${days} at ${formatTime(state.hour, state.minute)}`;
        }
        case 'monthly': {
          if (state.monthlyMode === 'each') {
            const days = state.monthDays.sort((a, b) => a - b).join(', ');
            return `Monthly on ${days} at ${formatTime(state.hour, state.minute)}`;
          }
          return `Monthly ${state.ordinalPosition} ${['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'][state.ordinalWeekday]} at ${formatTime(state.hour, state.minute)}`;
        }
        case 'custom':
          return `Cron ${schedule.expression}`;
      }
    }
  }
}

export function generateAutoName(action: Action, schedule: Schedule): string {
  return `${describeAction(action)} — ${describeSchedule(schedule)}`;
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /Users/marcus.grando/git/cronmac && npx tsc --noEmit --pretty 2>&1 | head -20`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/components/AutoName.ts
git commit -m "$(cat <<'EOF'
feat: update AutoName to produce better names from frequency-based schedules
EOF
)"
```
