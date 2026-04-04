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
