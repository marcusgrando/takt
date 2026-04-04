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
      // Extract label from pane_url
      const id = action.pane_url.split(':')[1] ?? '';
      const name = id.split('.').pop()?.replace('-Settings', '').replace('.extension', '').replace('-', ' ') || 'Settings';
      return `Open ${name}`;
    }
  }
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
