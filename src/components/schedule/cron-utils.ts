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
