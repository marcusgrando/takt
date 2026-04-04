import { Link, FileText, Terminal, Bell, Globe, Settings } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { Action, Schedule } from '@/lib/api';

export type TemplateName = 'OpenUrl' | 'OpenFile' | 'RunCommand' | 'Notify' | 'Webhook' | 'Custom';

export interface TemplateConfig {
  name: TemplateName;
  label: string;
  icon: React.ReactNode;
  action: Action;
  schedule: Schedule;
}

export const TEMPLATES: TemplateConfig[] = [
  {
    name: 'OpenUrl',
    label: 'Open URL',
    icon: <Link className="size-5" />,
    action: { type: 'OpenUrl', url: '', browser: undefined },
    schedule: { type: 'Cron', expression: '0 9 * * *' },
  },
  {
    name: 'OpenFile',
    label: 'Open File',
    icon: <FileText className="size-5" />,
    action: { type: 'OpenFile', path: '' },
    schedule: { type: 'Cron', expression: '0 9 * * *' },
  },
  {
    name: 'RunCommand',
    label: 'Run Command',
    icon: <Terminal className="size-5" />,
    action: { type: 'RunCommand', command: '', args: [], shell: 'Zsh' },
    schedule: { type: 'Cron', expression: '0 * * * *' },
  },
  {
    name: 'Notify',
    label: 'Reminder',
    icon: <Bell className="size-5" />,
    action: { type: 'Notify', title: '', body: '', sound: true },
    schedule: { type: 'Cron', expression: '0 9 * * *' },
  },
  {
    name: 'Webhook',
    label: 'Webhook',
    icon: <Globe className="size-5" />,
    action: { type: 'Webhook', url: '', method: 'GET', headers: {}, body: undefined },
    schedule: { type: 'Cron', expression: '0 * * * *' },
  },
  {
    name: 'Custom',
    label: 'Custom',
    icon: <Settings className="size-5" />,
    action: { type: 'OpenUrl', url: '', browser: undefined },
    schedule: { type: 'Cron', expression: '0 9 * * *' },
  },
];

interface TemplateGridProps {
  onSelect: (template: TemplateName) => void;
  onBack: () => void;
}

export default function TemplateGrid({ onSelect, onBack }: TemplateGridProps) {
  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center px-4 h-10 shrink-0">
        <Button variant="link" size="sm" onClick={onBack} className="px-0 text-primary">
          Back
        </Button>
        <span className="flex-1 text-center text-sm font-semibold">New Task</span>
        <div className="w-10" /> {/* spacer for centering */}
      </div>
      <div className="flex-1 overflow-y-auto px-4 py-3 scrollbar-none">
        <div className="grid grid-cols-2 gap-2">
          {TEMPLATES.map((tpl) => (
            <button
              key={tpl.name}
              onClick={() => onSelect(tpl.name)}
              className="flex flex-col items-center gap-2 rounded-lg border border-border bg-background p-4 text-sm font-medium transition-colors hover:bg-muted hover:border-primary/30 active:bg-muted/80"
            >
              <span className="text-muted-foreground">{tpl.icon}</span>
              {tpl.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
