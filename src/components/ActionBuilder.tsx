import { useState, useEffect } from 'react';
import { Plus, X, FolderOpen } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { type Action, type Shell, type HttpMethod, type KeyCombo, type Modifier, listBrowsers } from '@/lib/api';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Button } from '@/components/ui/button';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';

interface ActionBuilderProps {
  value: Action;
  onChange: (a: Action) => void;
}

const ACTION_TYPES: { type: Action['type']; label: string }[] = [
  { type: 'OpenUrl', label: 'URL' },
  { type: 'OpenFile', label: 'File' },
  { type: 'OpenApp', label: 'App' },
  { type: 'RunCommand', label: 'Cmd' },
  { type: 'Notify', label: 'Notify' },
  { type: 'Webhook', label: 'Hook' },
];

const SHELLS: Shell[] = ['Sh', 'Bash', 'Zsh', 'Python', 'AppleScript'];
const HTTP_METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];
const MODIFIERS: Modifier[] = ['Cmd', 'Shift', 'Opt', 'Ctrl'];

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
    case 'OpenFile': return { type: 'OpenFile', path: '', app: undefined, post_shortcuts: [] };
    case 'OpenUrl': return { type: 'OpenUrl', url: '', browser: undefined, post_shortcuts: [] };
    case 'OpenApp': return { type: 'OpenApp', app_path: '', post_shortcuts: [] };
    case 'RunCommand': return { type: 'RunCommand', command: '', args: [], shell: 'Zsh' };
    case 'Notify': return { type: 'Notify', title: '', body: '', sound: true };
    case 'Webhook': return { type: 'Webhook', url: '', method: 'GET', headers: {}, body: undefined };
  }
}

function PostShortcutsEditor({ shortcuts, onChange }: {
  shortcuts: KeyCombo[];
  onChange: (s: KeyCombo[]) => void;
}) {
  const [expanded, setExpanded] = useState(shortcuts.length > 0);

  function addCombo() {
    onChange([...shortcuts, { modifiers: [], key: '' }]);
    setExpanded(true);
  }

  function removeCombo(index: number) {
    onChange(shortcuts.filter((_, i) => i !== index));
  }

  function updateCombo(index: number, combo: KeyCombo) {
    onChange(shortcuts.map((c, i) => i === index ? combo : c));
  }

  function toggleModifier(index: number, mod: Modifier) {
    const combo = shortcuts[index];
    const has = combo.modifiers.includes(mod);
    const newMods = has
      ? combo.modifiers.filter((m) => m !== mod)
      : [...combo.modifiers, mod];
    updateCombo(index, { ...combo, modifiers: newMods });
  }

  if (!expanded && shortcuts.length === 0) {
    return (
      <button type="button" onClick={() => { addCombo(); }} className="text-sm text-primary hover:underline">
        + Run shortcuts after open
      </button>
    );
  }

  return (
    <div className="space-y-3">
      <Label className="text-xs font-medium uppercase tracking-widest text-muted-foreground">
        Shortcuts after open
      </Label>
      {shortcuts.map((combo, i) => (
        <div key={i} className="flex items-center gap-2">
          <div className="flex gap-1">
            {MODIFIERS.map((mod) => (
              <button
                key={mod}
                type="button"
                onClick={() => toggleModifier(i, mod)}
                className={`px-2 py-1 text-xs rounded border transition-colors ${
                  combo.modifiers.includes(mod)
                    ? 'bg-primary text-primary-foreground border-primary'
                    : 'bg-background border-border text-muted-foreground hover:border-primary/30'
                }`}
              >
                {mod}
              </button>
            ))}
          </div>
          <Input
            value={combo.key}
            onChange={(e) => updateCombo(i, { ...combo, key: e.target.value })}
            placeholder="key"
            className="w-20 font-mono"
          />
          <Button variant="ghost" size="icon-sm" onClick={() => removeCombo(i)} className="text-muted-foreground hover:text-destructive">
            <X className="size-3.5" />
          </Button>
        </div>
      ))}
      <Button variant="outline" size="sm" onClick={addCombo} className="w-full">
        <Plus className="size-3.5" />
        Add shortcut
      </Button>
    </div>
  );
}

async function pickFile(): Promise<string | null> {
  const result = await open({ multiple: false, directory: false });
  return result ?? null;
}

async function pickApp(): Promise<string | null> {
  const result = await open({
    multiple: false,
    directory: false,
    defaultPath: '/Applications',
  });
  return result ?? null;
}

export default function ActionBuilder({ value, onChange }: ActionBuilderProps) {
  const [browsers, setBrowsers] = useState<string[]>([]);

  useEffect(() => {
    listBrowsers().then(setBrowsers).catch(() => {});
  }, []);

  function handleTypeChange(v: string) {
    onChange(defaultAction(v as Action['type']));
  }

  const hasPostShortcuts = value.type === 'OpenFile' || value.type === 'OpenUrl' || value.type === 'OpenApp';
  const postShortcuts = hasPostShortcuts ? (value as { post_shortcuts: KeyCombo[] }).post_shortcuts : [];

  function handleShortcutsChange(shortcuts: KeyCombo[]) {
    onChange({ ...value, post_shortcuts: shortcuts } as Action);
  }

  return (
    <div className="space-y-4">
      <Tabs value={value.type} onValueChange={handleTypeChange}>
        <TabsList className="w-full flex-wrap h-auto gap-0 p-1">
          {ACTION_TYPES.map(({ type, label }) => (
            <TabsTrigger key={type} value={type} className="flex-1">{label}</TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      {value.type === 'OpenFile' && (
        <div className="space-y-3">
          <div className="space-y-2">
            <Label>File path</Label>
            <div className="flex gap-2">
              <Input
                value={value.path}
                onChange={(e) => onChange({ ...value, path: e.target.value })}
                placeholder="/path/to/file"
                className="font-mono flex-1"
              />
              <Button variant="outline" size="sm" onClick={async () => {
                const path = await pickFile();
                if (path) onChange({ ...value, path });
              }}>
                <FolderOpen className="size-4" />
              </Button>
            </div>
          </div>
          <div className="space-y-2">
            <Label>Open with app <span className="text-muted-foreground font-normal">(optional)</span></Label>
            <div className="flex gap-2">
              <Input
                value={value.app ?? ''}
                onChange={(e) => onChange({ ...value, app: e.target.value || undefined })}
                placeholder="Default app"
                className="flex-1"
              />
              <Button variant="outline" size="sm" onClick={async () => {
                const path = await pickApp();
                if (path) {
                  const name = path.split('/').pop()?.replace('.app', '') || path;
                  onChange({ ...value, app: name });
                }
              }}>
                <FolderOpen className="size-4" />
              </Button>
            </div>
          </div>
        </div>
      )}

      {value.type === 'OpenUrl' && (
        <div className="space-y-3">
          <div className="space-y-2">
            <Label>URL</Label>
            <Input value={value.url} onChange={(e) => onChange({ ...value, url: e.target.value })} placeholder="https://example.com" />
          </div>
          <div className="space-y-2">
            <Label>Browser <span className="text-muted-foreground font-normal">(optional)</span></Label>
            <Select value={value.browser ?? '__default__'} onValueChange={(v) => onChange({ ...value, browser: v === '__default__' ? undefined : v })}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="__default__">Default browser</SelectItem>
                {browsers.map((b) => <SelectItem key={b} value={b}>{b}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>
        </div>
      )}

      {value.type === 'OpenApp' && (
        <div className="space-y-2">
          <Label>Application</Label>
          <div className="flex gap-2">
            <Input
              value={value.app_path}
              onChange={(e) => onChange({ ...value, app_path: e.target.value })}
              placeholder="/Applications/App.app"
              className="font-mono flex-1"
            />
            <Button variant="outline" size="sm" onClick={async () => {
              const path = await pickApp();
              if (path) onChange({ ...value, app_path: path });
            }}>
              <FolderOpen className="size-4" />
            </Button>
          </div>
        </div>
      )}

      {value.type === 'RunCommand' && (
        <div className="space-y-3">
          <div className="space-y-2">
            <Label>Shell</Label>
            <Select value={value.shell} onValueChange={(v) => onChange({ ...value, shell: v as Shell })}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>{SHELLS.map((s) => <SelectItem key={s} value={s}>{s}</SelectItem>)}</SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label>Command</Label>
            <Input value={value.command} onChange={(e) => onChange({ ...value, command: e.target.value })} placeholder="echo hello" className="font-mono" />
          </div>
          <div className="space-y-2">
            <Label>Arguments <span className="text-muted-foreground font-normal">(one per line)</span></Label>
            <Textarea value={value.args.join('\n')} onChange={(e) => onChange({ ...value, args: e.target.value.split('\n').map((a) => a.trim()).filter(Boolean) })} placeholder={"--flag\nvalue"} className="font-mono resize-none" rows={3} />
          </div>
        </div>
      )}

      {value.type === 'Notify' && (
        <div className="space-y-3">
          <div className="space-y-2">
            <Label>Title</Label>
            <Input value={value.title} onChange={(e) => onChange({ ...value, title: e.target.value })} placeholder="Notification title" />
          </div>
          <div className="space-y-2">
            <Label>Body</Label>
            <Textarea value={value.body} onChange={(e) => onChange({ ...value, body: e.target.value })} placeholder="Notification body" className="resize-none" rows={3} />
          </div>
          <div className="flex items-center gap-2">
            <Switch id="n-sound" checked={value.sound} onCheckedChange={(checked) => onChange({ ...value, sound: checked })} />
            <Label htmlFor="n-sound">Play sound</Label>
          </div>
        </div>
      )}

      {value.type === 'Webhook' && (
        <div className="space-y-3">
          <div className="flex gap-2">
            <div className="space-y-2">
              <Label>Method</Label>
              <Select value={value.method} onValueChange={(v) => onChange({ ...value, method: v as HttpMethod })}>
                <SelectTrigger className="w-24"><SelectValue /></SelectTrigger>
                <SelectContent>{HTTP_METHODS.map((m) => <SelectItem key={m} value={m}>{m}</SelectItem>)}</SelectContent>
              </Select>
            </div>
            <div className="space-y-2 flex-1">
              <Label>URL</Label>
              <Input value={value.url} onChange={(e) => onChange({ ...value, url: e.target.value })} placeholder="https://api.example.com/hook" />
            </div>
          </div>
          <div className="space-y-2">
            <Label>Headers <span className="text-muted-foreground font-normal">(key=value per line)</span></Label>
            <Textarea value={headersToText(value.headers)} onChange={(e) => onChange({ ...value, headers: textToHeaders(e.target.value) })} placeholder="Authorization=Bearer token" className="font-mono resize-none" rows={3} />
          </div>
          <div className="space-y-2">
            <Label>Body <span className="text-muted-foreground font-normal">(optional)</span></Label>
            <Textarea value={value.body ?? ''} onChange={(e) => onChange({ ...value, body: e.target.value || undefined })} placeholder='{"key": "value"}' className="font-mono resize-none" rows={3} />
          </div>
        </div>
      )}

      {hasPostShortcuts && (
        <>
          <div className="border-t border-border pt-4" />
          <PostShortcutsEditor shortcuts={postShortcuts} onChange={handleShortcutsChange} />
        </>
      )}
    </div>
  );
}
