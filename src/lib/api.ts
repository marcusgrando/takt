import { invoke } from '@tauri-apps/api/core';

// ── Types ──────────────────────────────────────────────────────────────────

export type Shell = 'Sh' | 'Bash' | 'Zsh' | 'Python' | 'AppleScript';
export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';

export type Modifier = 'Cmd' | 'Shift' | 'Opt' | 'Ctrl';

export interface KeyCombo {
  modifiers: Modifier[];
  key: string;
}

export type Schedule =
  | { type: 'Cron'; expression: string }
  | { type: 'OneShot'; run_at: string } // ISO 8601
  | { type: 'DailyFirstUse'; delay_minutes: number };

export type Action =
  | { type: 'OpenFile'; path: string; app?: string; post_shortcuts: KeyCombo[]; shortcut_delay_secs: number }
  | { type: 'OpenUrl'; url: string; browser?: string; post_shortcuts: KeyCombo[]; shortcut_delay_secs: number }
  | { type: 'OpenApp'; app_path: string; post_shortcuts: KeyCombo[]; shortcut_delay_secs: number }
  | { type: 'RunCommand'; command: string; args: string[]; shell: Shell }
  | { type: 'Notify'; title: string; body: string; sound: boolean }
  | {
      type: 'Webhook';
      url: string;
      method: HttpMethod;
      headers: Record<string, string>;
      body?: string;
    }
  | { type: 'Settings'; pane_url: string };

export interface TaskDto {
  id: string;
  name: string;
  description?: string;
  enabled: boolean;
  run_if_missed: boolean;
  notify_on_run: boolean;
  schedule: Schedule;
  action: Action;
  created_at: string;
  updated_at: string;
  last_run_at?: string;
  next_run_at?: string;
}

export interface ExecutionLog {
  id: string;
  task_id: string;
  started_at: string;
  finished_at: string;
  status: 'success' | 'failure' | 'skipped';
  stdout?: string;
  stderr?: string;
  error?: string;
}

// ── Internal helper ────────────────────────────────────────────────────────

// Tauri invoke rejects with a string; normalize to Error so React Query
// (and any other caller) always receives a proper Error object.
async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(typeof e === 'string' ? e : String(e));
  }
}

// ── API functions ──────────────────────────────────────────────────────────

export async function listTasks(): Promise<TaskDto[]> {
  return tauriInvoke<TaskDto[]>('list_tasks');
}

export async function getTask(id: string): Promise<TaskDto | null> {
  return tauriInvoke<TaskDto | null>('get_task', { id });
}

export async function createTask(params: {
  name: string;
  description?: string;
  run_if_missed?: boolean;
  notify_on_run?: boolean;
  schedule: Schedule;
  action: Action;
}): Promise<TaskDto> {
  return tauriInvoke<TaskDto>('create_task', params);
}

export async function updateTask(params: {
  id: string;
  name?: string;
  description?: string | null; // null = clear, undefined = no change
  enabled?: boolean;
  run_if_missed?: boolean;
  notify_on_run?: boolean;
  schedule?: Schedule;
  action?: Action;
}): Promise<TaskDto> {
  return tauriInvoke<TaskDto>('update_task', params);
}

export async function deleteTask(id: string): Promise<void> {
  return tauriInvoke<void>('delete_task', { id });
}

export async function runTaskNow(id: string): Promise<void> {
  return tauriInvoke<void>('run_task_now', { id });
}

export async function listLogs(params?: {
  taskId?: string;
  limit?: number;
}): Promise<ExecutionLog[]> {
  return tauriInvoke<ExecutionLog[]>('list_logs', {
    task_id: params?.taskId,
    limit: params?.limit !== undefined ? Math.trunc(params.limit) : undefined,
  });
}

export async function listBrowsers(): Promise<string[]> {
  return tauriInvoke<string[]>('list_browsers');
}

export async function listAppsForFile(path: string): Promise<string[]> {
  return tauriInvoke<string[]>('list_apps_for_file', { path });
}
