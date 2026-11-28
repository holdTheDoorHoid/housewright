// Mirrors crates/api/src/dto.rs. Keep the two in step: the Rust test
// `every_listed_command_is_dispatched` and the vitest `commands.test.ts`
// both check the command list.

export type TaskState =
  | 'scheduled'
  | 'due_soon'
  | 'due'
  | 'overdue'
  | 'snoozed'
  | 'paused'
  | 'not_applicable'
  | 'completed';

export type PriorityClass = 'life_safety' | 'property_loss' | 'efficiency' | 'cosmetic';
export type Role = 'owner' | 'member' | 'limited';
export type Weekday =
  | 'monday'
  | 'tuesday'
  | 'wednesday'
  | 'thursday'
  | 'friday'
  | 'saturday'
  | 'sunday';

/** Civil date, "2026-11-27". */
export type DateStr = string;
/** Clock time, "22:00" or "22:00:00". */
export type TimeStr = string;

export interface TimeRange {
  start: TimeStr;
  end: TimeStr;
}

export interface MaintenanceWindow {
  days: Weekday[];
  start: TimeStr;
  end: TimeStr;
}

export interface ReminderPrefs {
  quiet_hours?: TimeRange | null;
  maintenance_window?: MaintenanceWindow | null;
}

export interface ApiError {
  code: 'invalid' | 'not_found' | 'not_initialized' | 'import' | 'internal' | 'locked' | string;
  message: string;
}

export interface Member {
  id: string;
  name: string;
  role: Role;
  reminders: ReminderPrefs;
}

export interface PropertyInfo {
  id: string;
  name: string;
  time_zone: string;
}

export interface Status {
  initialized: boolean;
  household_name: string | null;
  me: Member | null;
  property: PropertyInfo | null;
}

export interface Card {
  id: string;
  title: string;
  item_id: string | null;
  item_name: string | null;
  room_name: string | null;
  state: TaskState;
  state_label: string;
  priority: PriorityClass;
  priority_label: string;
  due: DateStr | null;
  earliest: DateStr | null;
  overdue_after: DateStr | null;
  last_done: DateStr | null;
  snooze_count: number;
  needs_review: boolean;
  minutes: number | null;
  owner_id: string;
  owner_name: string;
  active: boolean;
  schedule_text: string;
}

export interface Today {
  tasks: Card[];
  coming_up: Card[];
  lapsed: boolean;
  welcome_back: { focus: Card[]; others_overdue: number } | null;
  active_count: number;
  suggested_count: number;
}

export interface CompletionInfo {
  id: string;
  task_id: string;
  task_title: string;
  at: string;
  by_name: string;
  notes: string[];
  voided: boolean;
}

export interface TaskDetail {
  card: Card;
  steps: string[];
  why: string | null;
  source: string | null;
  interval: string | null;
  paused: boolean;
  not_applicable: boolean;
  completions: CompletionInfo[];
}

export interface SnoozeOption {
  key: string;
  label: string;
  date: DateStr;
}

export interface ItemRow {
  id: string;
  name: string;
  category: string | null;
  room_id: string | null;
  room_name: string | null;
  template: string | null;
  state: TaskState | null;
  next_due: DateStr | null;
}

export interface ItemDetail {
  row: ItemRow;
  make: string | null;
  model: string | null;
  serial: string | null;
  installed: DateStr | null;
  purchased: DateStr | null;
  fields: { key: string; label: string; value: string; sensitive: boolean }[];
  tasks: Card[];
  history: CompletionInfo[];
}

export interface Room {
  id: string;
  name: string;
}

export type FieldDef = {
  key: string;
  label: string;
  sensitivity?: 'normal' | 'sensitive';
  help?: string | null;
} & (
  | { type: 'text' }
  | { type: 'number'; unit?: string | null }
  | { type: 'date' }
  | { type: 'yes_no' }
  | { type: 'choice'; options: string[] }
  | { type: 'photo' }
);

export interface TemplateSummary {
  id: string;
  name: string;
  category: string | null;
  description: string | null;
  fields: FieldDef[];
  tasks: { key: string; title: string; priority: PriorityClass; schedule_text: string }[];
}

export interface HistoryEntry {
  at: string;
  author: string;
  text: string;
}

export interface Reminder {
  task_id: string;
  kind: 'due' | 'follow_up' | 'rescheduled';
  at_ms: number;
  title: string;
  body: string;
}

type None = Record<string, never> | undefined;

/** Every command: [arguments, result]. */
export interface Commands {
  status: [None, Status];
  init: [
    { household_name?: string; property_name: string; your_name: string; time_zone: string },
    Status,
  ];
  today: [None, Today];
  mark_seen: [None, null];
  task: [{ id: string }, TaskDetail];
  complete: [{ id: string; notes?: string; at_ms?: number }, { completion_id: string; card: Card }];
  annotate: [{ completion_id: string; text: string }, null];
  void_completion: [{ id: string }, Card];
  snooze_options: [None, SnoozeOption[]];
  snooze: [{ id: string; until: DateStr; reason?: string }, Card];
  review: [
    {
      id: string;
      action: 'change_interval' | 'reassign' | 'not_applicable' | 'pause' | 'resume';
      every?: string;
      member?: string;
    },
    Card,
  ];
  set_active: [{ id: string; active: boolean }, Card];
  items: [None, ItemRow[]];
  item: [{ id: string }, ItemDetail];
  rooms: [None, Room[]];
  add_room: [{ name: string }, Room];
  members: [None, Member[]];
  templates: [None, TemplateSummary[]];
  add_from_template: [
    {
      template: string;
      name?: string;
      room_id?: string;
      new_room?: string;
      make?: string;
      model?: string;
      installed?: DateStr;
      values?: Record<string, string>;
    },
    { item_id: string; tasks: Card[] },
  ];
  start_small: [{ item_id?: string; n?: number }, Card[]];
  history: [None, HistoryEntry[]];
  set_reminder_prefs: [{ member_id?: string; prefs: ReminderPrefs }, Member];
  reminder_plan: [{ horizon_days?: number }, Reminder[]];
  test_reminder: [{ task_id?: string; in_seconds?: number }, Reminder];
  export: [None, { filename: string; json: string }];
  import: [{ json: string }, Status];
}

export type Command = keyof Commands;

export const COMMANDS: Command[] = [
  'status',
  'init',
  'today',
  'mark_seen',
  'task',
  'complete',
  'annotate',
  'void_completion',
  'snooze_options',
  'snooze',
  'review',
  'set_active',
  'items',
  'item',
  'rooms',
  'add_room',
  'members',
  'templates',
  'add_from_template',
  'start_small',
  'history',
  'set_reminder_prefs',
  'reminder_plan',
  'test_reminder',
  'export',
  'import',
];
