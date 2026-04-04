import { type Action, type Shell, type HttpMethod } from '@/lib/api';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';

interface ActionBuilderProps {
  value: Action;
  onChange: (a: Action) => void;
}

const ACTION_TYPES: { type: Action['type']; label: string }[] = [
  { type: 'OpenFile', label: 'File' },
  { type: 'OpenUrl', label: 'URL' },
  { type: 'RunCommand', label: 'Cmd' },
  { type: 'Notify', label: 'Notify' },
  { type: 'Shortcut', label: 'Keys' },
  { type: 'Webhook', label: 'Hook' },
];

const SHELLS: Shell[] = ['Sh', 'Bash', 'Zsh', 'Python', 'AppleScript'];
const HTTP_METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];

function headersToText(headers: Record<string, string>): string {
  return Object.entries(headers).map(([k, v]) => `${k}=${v}`).join('\n');
}

function textToHeaders(text: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const line of text.split('\n')) {
    const idx = line.indexOf('=');
    if (idx < 1) continue;
    result[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
  }
  return result;
}

function defaultAction(type: Action['type']): Action {
  switch (type) {
    case 'OpenFile': return { type: 'OpenFile', path: '' };
    case 'OpenUrl': return { type: 'OpenUrl', url: '', browser: undefined };
    case 'RunCommand': return { type: 'RunCommand', command: '', args: [], shell: 'Zsh' };
    case 'Notify': return { type: 'Notify', title: '', body: '', sound: false };
    case 'Shortcut': return { type: 'Shortcut', keys: [] };
    case 'Webhook': return { type: 'Webhook', url: '', method: 'GET', headers: {}, body: undefined };
  }
}

export default function ActionBuilder({ value, onChange }: ActionBuilderProps) {
  function handleTypeChange(v: string) {
    onChange(defaultAction(v as Action['type']));
  }

  return (
    <div className="space-y-4">
      {/* Action type — shadcn Tabs */}
      <Tabs value={value.type} onValueChange={handleTypeChange}>
        <TabsList className="w-full flex-wrap h-auto gap-0 p-1">
          {ACTION_TYPES.map(({ type, label }) => (
            <TabsTrigger key={type} value={type} className="flex-1">{label}</TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      {value.type === 'OpenFile' && (
        <div className="space-y-2">
          <Label htmlFor="of-path">File path</Label>
          <Input id="of-path" value={value.path} onChange={(e) => onChange({ ...value, path: e.target.value })} placeholder="/path/to/file" className="font-mono" />
        </div>
      )}

      {value.type === 'OpenUrl' && (
        <div className="space-y-3">
          <div className="space-y-2">
            <Label htmlFor="ou-url">URL</Label>
            <Input id="ou-url" value={value.url} onChange={(e) => onChange({ ...value, url: e.target.value })} placeholder="https://example.com" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="ou-browser">Browser <span className="text-muted-foreground font-normal">(optional)</span></Label>
            <Input id="ou-browser" value={value.browser ?? ''} onChange={(e) => onChange({ ...value, browser: e.target.value || undefined })} placeholder="Safari, Firefox, …" />
          </div>
        </div>
      )}

      {value.type === 'RunCommand' && (
        <div className="space-y-3">
          <div className="space-y-2">
            <Label htmlFor="rc-shell">Shell</Label>
            <Select value={value.shell} onValueChange={(v) => onChange({ ...value, shell: v as Shell })}>
              <SelectTrigger id="rc-shell"><SelectValue /></SelectTrigger>
              <SelectContent>{SHELLS.map((s) => <SelectItem key={s} value={s}>{s}</SelectItem>)}</SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label htmlFor="rc-cmd">Command</Label>
            <Input id="rc-cmd" value={value.command} onChange={(e) => onChange({ ...value, command: e.target.value })} placeholder="echo hello" className="font-mono" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="rc-args">Arguments <span className="text-muted-foreground font-normal">(one per line)</span></Label>
            <Textarea id="rc-args" value={value.args.join('\n')} onChange={(e) => onChange({ ...value, args: e.target.value.split('\n').map((a) => a.trim()).filter(Boolean) })} placeholder={"--flag\nvalue"} className="font-mono resize-none" rows={3} />
          </div>
        </div>
      )}

      {value.type === 'Notify' && (
        <div className="space-y-3">
          <div className="space-y-2">
            <Label htmlFor="n-title">Title</Label>
            <Input id="n-title" value={value.title} onChange={(e) => onChange({ ...value, title: e.target.value })} placeholder="Notification title" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="n-body">Body</Label>
            <Textarea id="n-body" value={value.body} onChange={(e) => onChange({ ...value, body: e.target.value })} placeholder="Notification body" className="resize-none" rows={3} />
          </div>
          <div className="flex items-center gap-2">
            <Switch id="n-sound" checked={value.sound} onCheckedChange={(checked) => onChange({ ...value, sound: checked })} />
            <Label htmlFor="n-sound">Play sound</Label>
          </div>
        </div>
      )}

      {value.type === 'Shortcut' && (
        <div className="space-y-2">
          <Label htmlFor="sc-keys">Keys <span className="text-muted-foreground font-normal">(comma-separated)</span></Label>
          <Input id="sc-keys" value={value.keys.join(', ')} onChange={(e) => onChange({ ...value, keys: e.target.value.split(',').map((k) => k.trim()).filter(Boolean) })} placeholder="cmd, shift, 4" className="font-mono" />
        </div>
      )}

      {value.type === 'Webhook' && (
        <div className="space-y-3">
          <div className="flex gap-2">
            <div className="space-y-2">
              <Label htmlFor="wh-method">Method</Label>
              <Select value={value.method} onValueChange={(v) => onChange({ ...value, method: v as HttpMethod })}>
                <SelectTrigger id="wh-method" className="w-24"><SelectValue /></SelectTrigger>
                <SelectContent>{HTTP_METHODS.map((m) => <SelectItem key={m} value={m}>{m}</SelectItem>)}</SelectContent>
              </Select>
            </div>
            <div className="space-y-2 flex-1">
              <Label htmlFor="wh-url">URL</Label>
              <Input id="wh-url" value={value.url} onChange={(e) => onChange({ ...value, url: e.target.value })} placeholder="https://api.example.com/hook" />
            </div>
          </div>
          <div className="space-y-2">
            <Label htmlFor="wh-headers">Headers <span className="text-muted-foreground font-normal">(key=value per line)</span></Label>
            <Textarea id="wh-headers" value={headersToText(value.headers)} onChange={(e) => onChange({ ...value, headers: textToHeaders(e.target.value) })} placeholder="Authorization=Bearer token" className="font-mono resize-none" rows={3} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="wh-body">Body <span className="text-muted-foreground font-normal">(optional)</span></Label>
            <Textarea id="wh-body" value={value.body ?? ''} onChange={(e) => onChange({ ...value, body: e.target.value || undefined })} placeholder='{"key": "value"}' className="font-mono resize-none" rows={3} />
          </div>
        </div>
      )}
    </div>
  );
}
