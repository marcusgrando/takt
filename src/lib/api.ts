import { invoke } from '@tauri-apps/api/core';

// ── Types ──────────────────────────────────────────────────────────────────

export type Shell = 'Sh' | 'Bash' | 'Zsh' | 'Python' | 'AppleScript';
export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';

export type Schedule =
  | { type: 'Cron'; expression: string }
  | { type: 'OneShot'; run_at: string } // ISO 8601
  | { type: 'OnLogin' }
  | { type: 'OnWake' };

export type Action =
  | { type: 'OpenFile'; path: string }
  | { type: 'OpenUrl'; url: string; browser?: string }
  | { type: 'RunCommand'; command: string; args: string[]; shell: Shell }
  | { type: 'Notify'; title: string; body: string; sound: boolean }
  | { type: 'Shortcut'; keys: string[] }
  | {
      type: 'Webhook';
      url: string;
      method: HttpMethod;
      headers: Record<string, string>;
      body?: string;
    };

export interface TaskDto {
  id: string;
  name: string;
  description?: string;
  enabled: boolean;
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

// ── API functions ──────────────────────────────────────────────────────────

export async function listTasks(): Promise<TaskDto[]> {
  return invoke('list_tasks');
}

export async function getTask(id: string): Promise<TaskDto | null> {
  return invoke('get_task', { id });
}

export async function createTask(params: {
  name: string;
  description?: string;
  schedule: Schedule;
  action: Action;
}): Promise<TaskDto> {
  return invoke('create_task', params);
}

export async function updateTask(params: {
  id: string;
  name?: string;
  description?: string | null; // null = clear, undefined = no change
  enabled?: boolean;
  schedule?: Schedule;
  action?: Action;
}): Promise<TaskDto> {
  return invoke('update_task', params);
}

export async function deleteTask(id: string): Promise<void> {
  return invoke('delete_task', { id });
}

export async function runTaskNow(id: string): Promise<void> {
  return invoke('run_task_now', { id });
}

export async function listLogs(params?: {
  taskId?: string;
  limit?: number;
}): Promise<ExecutionLog[]> {
  return invoke('list_logs', {
    task_id: params?.taskId,
    limit: params?.limit,
  });
}
