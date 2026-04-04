import { type Action, type Shell, type HttpMethod } from '@/lib/api';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';

interface ActionBuilderProps {
  value: Action;
  onChange: (a: Action) => void;
}

const ACTION_TYPES: { type: Action['type']; label: string }[] = [
  { type: 'OpenFile',   label: 'File' },
  { type: 'OpenUrl',    label: 'URL' },
  { type: 'RunCommand', label: 'Command' },
  { type: 'Notify',     label: 'Notify' },
  { type: 'Shortcut',   label: 'Shortcut' },
  { type: 'Webhook',    label: 'Webhook' },
];

const SHELLS: Shell[] = ['Sh', 'Bash', 'Zsh', 'Python', 'AppleScript'];
const HTTP_METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];

/** Convert headers Record<string,string> → "key=value\nkey=value" */
function headersToText(headers: Record<string, string>): string {
  return Object.entries(headers)
    .map(([k, v]) => `${k}=${v}`)
    .join('\n');
}

/** Parse "key=value\n..." → Record<string,string> */
function textToHeaders(text: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const line of text.split('\n')) {
    const idx = line.indexOf('=');
    if (idx < 1) continue;
    const k = line.slice(0, idx).trim();
    const v = line.slice(idx + 1).trim();
    if (k) result[k] = v;
  }
  return result;
}

function defaultAction(type: Action['type']): Action {
  switch (type) {
    case 'OpenFile':   return { type: 'OpenFile', path: '' };
    case 'OpenUrl':    return { type: 'OpenUrl', url: '', browser: undefined };
    case 'RunCommand': return { type: 'RunCommand', command: '', args: [], shell: 'Zsh' };
    case 'Notify':     return { type: 'Notify', title: '', body: '', sound: false };
    case 'Shortcut':   return { type: 'Shortcut', keys: [] };
    case 'Webhook':    return { type: 'Webhook', url: '', method: 'GET', headers: {}, body: undefined };
  }
}

export default function ActionBuilder({ value, onChange }: ActionBuilderProps) {
  function handleTypeChange(type: Action['type']) {
    onChange(defaultAction(type));
  }

  return (
    <div className="flex flex-col gap-3">
      {/* Type tabs */}
      <div className="flex gap-1 rounded-lg bg-muted p-0.5">
        {ACTION_TYPES.map(({ type, label }) => (
          <button
            key={type}
            type="button"
            onClick={() => handleTypeChange(type)}
            className={[
              'flex-1 rounded-md px-1 py-1 text-[11px] font-medium transition-colors',
              value.type === type
                ? 'bg-background text-foreground shadow-sm'
                : 'text-muted-foreground hover:text-foreground',
            ].join(' ')}
          >
            {label}
          </button>
        ))}
      </div>

      {/* OpenFile */}
      {value.type === 'OpenFile' && (
        <div className="flex flex-col gap-1">
          <Label htmlFor="of-path" className="text-xs text-muted-foreground">File path</Label>
          <Input
            id="of-path"
            value={value.path}
            onChange={(e) => onChange({ ...value, path: e.target.value })}
            placeholder="/path/to/file"
            className="h-7 text-xs font-mono"
          />
        </div>
      )}

      {/* OpenUrl */}
      {value.type === 'OpenUrl' && (
        <div className="flex flex-col gap-2">
          <div className="flex flex-col gap-1">
            <Label htmlFor="ou-url" className="text-xs text-muted-foreground">URL</Label>
            <Input
              id="ou-url"
              value={value.url}
              onChange={(e) => onChange({ ...value, url: e.target.value })}
              placeholder="https://example.com"
              className="h-7 text-xs"
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="ou-browser" className="text-xs text-muted-foreground">
              Browser <span className="opacity-50">(optional)</span>
            </Label>
            <Input
              id="ou-browser"
              value={value.browser ?? ''}
              onChange={(e) =>
                onChange({ ...value, browser: e.target.value || undefined })
              }
              placeholder="Safari, Firefox, …"
              className="h-7 text-xs"
            />
          </div>
        </div>
      )}

      {/* RunCommand */}
      {value.type === 'RunCommand' && (
        <div className="flex flex-col gap-2">
          <div className="flex flex-col gap-1">
            <Label htmlFor="rc-shell" className="text-xs text-muted-foreground">Shell</Label>
            <Select
              value={value.shell}
              onValueChange={(v) => onChange({ ...value, shell: v as Shell })}
            >
              <SelectTrigger id="rc-shell" size="sm" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {SHELLS.map((s) => (
                  <SelectItem key={s} value={s}>{s}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="rc-cmd" className="text-xs text-muted-foreground">Command</Label>
            <Input
              id="rc-cmd"
              value={value.command}
              onChange={(e) => onChange({ ...value, command: e.target.value })}
              placeholder="echo hello"
              className="h-7 text-xs font-mono"
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="rc-args" className="text-xs text-muted-foreground">
              Args <span className="opacity-50">(comma-separated)</span>
            </Label>
            <Input
              id="rc-args"
              value={value.args.join(', ')}
              onChange={(e) =>
                onChange({
                  ...value,
                  args: e.target.value
                    .split(',')
                    .map((a) => a.trim())
                    .filter(Boolean),
                })
              }
              placeholder="--flag, value"
              className="h-7 text-xs font-mono"
            />
          </div>
        </div>
      )}

      {/* Notify */}
      {value.type === 'Notify' && (
        <div className="flex flex-col gap-2">
          <div className="flex flex-col gap-1">
            <Label htmlFor="n-title" className="text-xs text-muted-foreground">Title</Label>
            <Input
              id="n-title"
              value={value.title}
              onChange={(e) => onChange({ ...value, title: e.target.value })}
              placeholder="Notification title"
              className="h-7 text-xs"
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="n-body" className="text-xs text-muted-foreground">Body</Label>
            <Textarea
              id="n-body"
              value={value.body}
              onChange={(e) => onChange({ ...value, body: e.target.value })}
              placeholder="Notification body"
              className="min-h-14 text-xs resize-none"
              rows={3}
            />
          </div>
          <div className="flex items-center gap-2">
            <Switch
              id="n-sound"
              checked={value.sound}
              onCheckedChange={(checked) => onChange({ ...value, sound: checked })}
              size="sm"
            />
            <Label htmlFor="n-sound" className="text-xs">Play sound</Label>
          </div>
        </div>
      )}

      {/* Shortcut */}
      {value.type === 'Shortcut' && (
        <div className="flex flex-col gap-1">
          <Label htmlFor="sc-keys" className="text-xs text-muted-foreground">
            Keys <span className="opacity-50">(comma-separated, e.g. cmd, shift, 4)</span>
          </Label>
          <Input
            id="sc-keys"
            value={value.keys.join(', ')}
            onChange={(e) =>
              onChange({
                ...value,
                keys: e.target.value
                  .split(',')
                  .map((k) => k.trim())
                  .filter(Boolean),
              })
            }
            placeholder="cmd, shift, 4"
            className="h-7 text-xs font-mono"
          />
        </div>
      )}

      {/* Webhook */}
      {value.type === 'Webhook' && (
        <div className="flex flex-col gap-2">
          <div className="flex gap-2">
            <div className="flex flex-col gap-1">
              <Label htmlFor="wh-method" className="text-xs text-muted-foreground">Method</Label>
              <Select
                value={value.method}
                onValueChange={(v) => onChange({ ...value, method: v as HttpMethod })}
              >
                <SelectTrigger id="wh-method" size="sm" className="w-24">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {HTTP_METHODS.map((m) => (
                    <SelectItem key={m} value={m}>{m}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex flex-col gap-1 flex-1">
              <Label htmlFor="wh-url" className="text-xs text-muted-foreground">URL</Label>
              <Input
                id="wh-url"
                value={value.url}
                onChange={(e) => onChange({ ...value, url: e.target.value })}
                placeholder="https://api.example.com/hook"
                className="h-7 text-xs"
              />
            </div>
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="wh-headers" className="text-xs text-muted-foreground">
              Headers <span className="opacity-50">(key=value, one per line)</span>
            </Label>
            <Textarea
              id="wh-headers"
              value={headersToText(value.headers)}
              onChange={(e) =>
                onChange({ ...value, headers: textToHeaders(e.target.value) })
              }
              placeholder="Authorization=Bearer token"
              className="min-h-14 text-xs font-mono resize-none"
              rows={3}
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="wh-body" className="text-xs text-muted-foreground">
              Body <span className="opacity-50">(optional)</span>
            </Label>
            <Textarea
              id="wh-body"
              value={value.body ?? ''}
              onChange={(e) =>
                onChange({ ...value, body: e.target.value || undefined })
              }
              placeholder='{"key": "value"}'
              className="min-h-14 text-xs font-mono resize-none"
              rows={3}
            />
          </div>
        </div>
      )}
    </div>
  );
}
