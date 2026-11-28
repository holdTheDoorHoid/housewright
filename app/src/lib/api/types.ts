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
  /** ISO 4217, e.g. "USD". */
  currency: string | null;
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
  /** This week's single suggestion, if any (P3). */
  suggestion: Suggestion | null;
  /** Whether the home questions have been answered. */
  has_profile: boolean;
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
  attachments: AttachmentInfo[];
  costs: { kind: CostKind; minor: number; currency: string }[];
  cost_total: number | null;
  values: FieldInfo[];
}

export type CostKind = 'parts' | 'labour' | 'contractor' | 'other';
export type AttachmentKind = 'manual' | 'receipt' | 'warranty' | 'photo' | 'other';

export interface AttachmentInfo {
  id: string;
  name: string;
  media_type: string;
  size: number;
  kind: AttachmentKind;
  has_thumb: boolean;
  width: number | null;
  height: number | null;
  at: string | null;
  sensitive: boolean;
}

/** A field value for display; sensitive values come back empty. */
export interface FieldInfo {
  key: string;
  label: string;
  value: string;
  sensitive: boolean;
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
  completion_fields: FieldDef[];
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
  fields: FieldInfo[];
  tasks: Card[];
  history: CompletionInfo[];
  /** Sticker code, "HW-7K3QM". */
  code: string;
  next: Card | null;
  attachments: AttachmentInfo[];
  edit: FieldEdit[];
  serial_sensitive: boolean;
}

export interface FieldEdit {
  def: FieldDef;
  origin: 'template' | 'household' | 'item';
  value: string | null;
  has_value: boolean;
  sensitive: boolean;
  photo: AttachmentInfo | null;
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

/** Answers to the home questions (core `Profile` without its id). Absent = not sure. */
export interface ProfileAnswers {
  home_type?: 'house' | 'townhouse' | 'condo' | 'apartment' | 'mobile';
  built?: 'before_1950' | '1950_1979' | '1980_1999' | '2000_2019' | '2020_on';
  /** Sent as a plain string; stored flagged (`{ value, sensitivity }`). */
  zip?: string;
  heating?: ('gas_furnace' | 'oil' | 'heat_pump' | 'electric' | 'boiler' | 'other')[];
  cooling?: ('central_ac' | 'heat_pump' | 'window_units' | 'none')[];
  water_heater?: 'tank_gas' | 'tank_electric' | 'tankless' | 'heat_pump';
  water_supply?: 'city' | 'well';
  sewer?: 'city' | 'septic';
  features?: Record<string, boolean>;
  smoke_alarms?: 'none' | 'one' | 'two_to_three' | 'four_or_more';
  smoke_power?: 'battery' | 'hardwired' | 'sealed';
  co_alarms?: 'none' | 'one' | 'two_to_three' | 'four_or_more';
  people?: { adults?: number; children?: number; older_adults?: number; pets?: boolean };
}

export interface StoredProfile extends Omit<ProfileAnswers, 'zip'> {
  id: string;
  zip?: { value: string; sensitivity: 'normal' | 'sensitive' };
}

export interface ClimateInfo {
  lookup: {
    zip: string;
    matched_zip: string;
    approximate: boolean;
    climate: { hemisphere: 'north' | 'south'; first_frost_doy?: number; last_frost_doy?: number; hard_freeze?: boolean };
  };
  first_frost: string | null;
  last_frost: string | null;
  freezes: boolean;
}

export interface PlanTask {
  card: Card;
  why: string | null;
  source: string | null;
}

export interface StartingPlan {
  switched_on: PlanTask[];
  things_recorded: number;
  new_things: number;
  suggested_count: number;
  climate: ClimateInfo | null;
}

export interface SuggestedGroup {
  item_id: string | null;
  item_name: string;
  category: string | null;
  tasks: PlanTask[];
}

export interface Suggestion {
  key: string;
  action:
    | { kind: 'add_thing'; template: string; name: string; season?: string }
    | { kind: 'switch_on'; task: string };
  title: string;
  body: string;
  why: string;
  source: string;
  add_label: string;
  priority: PriorityClass;
  seasonal: boolean;
}

export type SuggestionAnswer = 'added' | 'not_for_us' | 'later';

export interface Split {
  parts: number;
  labour: number;
  contractor: number;
  other: number;
  total: number;
}

export interface CostRow<K> {
  key: K;
  label: string;
  split: Split;
  jobs: number;
}

export interface CostSummary {
  currency: string;
  total: Split;
  by_year: CostRow<number>[];
  by_item: CostRow<string | null>[];
  by_property: CostRow<string>[];
  other_currency_lines: number;
}

export interface FactInfo {
  id: string;
  group: string;
  label: string;
  value: string | null;
  sensitive: boolean;
  key: string | null;
  room_id: string | null;
  room_name: string | null;
  attachments: AttachmentInfo[];
}

export interface FactPrompt {
  key: string;
  group: string;
  label: string;
  question: string;
  hint: string;
  sensitive: boolean;
}

export interface Facts {
  property_name: string;
  groups: { key: string; label: string; sensitive_by_default: boolean; facts: FactInfo[] }[];
  prompts: FactPrompt[];
}

export interface SheetLayout {
  page_width: number;
  page_height: number;
  columns: number;
  rows: number;
  label_width: number;
  label_height: number;
  margin_left: number;
  margin_top: number;
  gap_x: number;
  gap_y: number;
  outline: boolean;
}

export interface Sticker {
  item_id: string;
  name: string;
  room_name: string | null;
  code: string;
  url: string;
  qr_svg: string;
}

export interface LabelRead {
  lines: string[];
  brand: string | null;
  model: string | null;
  serial: string | null;
  model_alternatives: string[];
  serial_alternatives: string[];
  template: { id: string; name: string } | null;
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
      serial?: string;
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
  climate_lookup: [{ zip: string }, ClimateInfo | null];
  profile: [None, { profile: StoredProfile | null; climate: ClimateInfo | null }];
  onboard: [{ profile: ProfileAnswers }, StartingPlan];
  starting_plan: [None, StartingPlan];
  all_suggested: [None, SuggestedGroup[]];
  answer_suggestion: [{ key: string; answer: SuggestionAnswer }, { switched_on: Card[] }];
  attachments: [{ about_kind: AboutKind; about_id: string }, AttachmentInfo[]];
  delete_attachment: [{ id: string }, { freed: number }];
  update_item: [
    {
      id: string;
      name?: string;
      make?: string;
      model?: string;
      serial?: string;
      installed?: string;
      purchased?: string;
      room_id?: string;
      values?: Record<string, string | null>;
    },
    { detail: ItemDetail; offered: Card[]; no_longer_apply: Card[] },
  ];
  add_field: [
    {
      item_id: string;
      to_template?: boolean;
      label: string;
      type: FieldDef['type'];
      unit?: string;
      options?: string[];
      sensitive?: boolean;
    },
    ItemDetail,
  ];
  remove_field: [{ item_id: string; key: string }, ItemDetail];
  reveal_field: [{ item_id: string; key: string }, { value: string }];
  add_details: [
    {
      completion_id: string;
      note?: string;
      costs?: Partial<Record<CostKind, string>>;
      values?: Record<string, string>;
    },
    TaskDetail,
  ];
  facts: [None, Facts];
  save_fact: [
    {
      id?: string;
      group: string;
      label: string;
      value?: string;
      sensitive?: boolean;
      key?: string;
      room_id?: string;
    },
    FactInfo,
  ];
  delete_fact: [{ id: string }, null];
  reveal_fact: [{ id: string }, { value: string }];
  costs: [None, CostSummary];
  set_currency: [{ currency: string }, Status];
  label_sheet: [{ item_ids?: string[] }, { presets: { key: string; label: string; layout: SheetLayout }[]; stickers: Sticker[] }];
  find_label: [{ text: string }, ItemRow[]];
  binder: [{ property_id?: string; include_costs?: boolean; include_notes?: boolean } | None, BinderInfo];
  set_sealed_location: [{ text: string }, BinderInfo];
  set_binder_reminder: [{ on: boolean }, BinderInfo];
  set_paper: [{ paper: Paper }, BinderInfo];
  /** The web's binder module records its print with this (D-80). */
  record_binder_print: [BinderReceipt, BinderInfo];
  notes: [{ about_kind: NoteAbout; about_id: string }, NoteInfo[]];
  add_note: [{ about_kind: NoteAbout; about_id: string; text: string; sensitive?: boolean }, NoteInfo[]];
  delete_note: [{ id: string }, null];
  contractors: [None, ContractorInfo[]];
  save_contractor: [ContractorInfo, ContractorInfo[]];
  delete_contractor: [{ id: string }, null];
  // -- sync (Phase 6) --
  sync_status: [None, SyncStatus];
  sync_check_server: [{ server: string }, { ok: boolean; version: string; time_ms: number; registration_open: boolean }];
  sync_setup: [{ server: string; device_name: string; registration_token?: string }, RecoveryKeyOut];
  sync_confirm_recovery: [None, SyncStatus];
  sync_now: [None, { report: SyncReport; status: SyncStatus }];
  sync_devices: [None, SyncDevice[]];
  sync_invite: [{ who: 'device' | 'person'; name?: string; role?: Role }, Invitation];
  sync_invite_poll: [None, PairState];
  sync_invite_confirm: [{ matches: boolean }, PairState];
  sync_invite_cancel: [None, null];
  sync_join: [{ server?: string; code: string; device_name: string }, PairState];
  sync_join_poll: [None, PairState];
  sync_join_confirm: [{ matches: boolean }, PairState];
  sync_restore: [{ server: string; recovery_key: string; device_name: string }, Status];
  sync_remove_device: [{ id: string; recovery_key?: string }, SyncDevice[]];
  sync_new_recovery_key: [{ recovery_key: string }, RecoveryKeyOut];
  sync_stop: [None, SyncStatus];
  sync_dismiss_notices: [None, SyncStatus];
}

// -- sync (Phase 6) ---------------------------------------------------------

export interface SyncStatus {
  /** This build can sync at all. */
  available: boolean;
  enabled: boolean;
  server: string | null;
  role: 'owner' | 'member' | 'limited' | 'recovery' | null;
  last_sync: string | null;
  last_error: string | null;
  last_error_code: string | null;
  clock_warning: string | null;
  clock_ahead: boolean;
  removed: boolean;
  pending_changes: number;
  recovery_confirmed: boolean;
  refused: number;
  /** Changes to the household's devices made on other devices. */
  notices: { at: string | null; text: string }[];
  warnings: string[];
}

export interface SyncReport {
  pushed_ops: number;
  pulled_ops: number;
  rejected_batches: number;
  refused_future_ops: number;
  files_up: number;
  files_down: number;
}

export interface RecoveryKeyOut {
  /** Shown once; never stored on the device. */
  recovery_key: string;
  status: SyncStatus;
}

export interface SyncDevice {
  id: string;
  name: string;
  member: string;
  member_name: string;
  role: 'owner' | 'member' | 'limited';
  this_device: boolean;
  added: string | null;
  last_seen: string | null;
}

export interface Invitation {
  code: string;
  server: string;
  qr_text: string;
  qr_svg: string;
  expires: string;
  expires_ms: number;
}

export type PairState =
  | { state: 'waiting' }
  | { state: 'compare'; safety_number: string; other_name: string; other_confirmed: boolean }
  | { state: 'waiting_for_other' }
  | { state: 'done' }
  | { state: 'failed'; reason: string };

// -- the house binder (Phase 5) -------------------------------------------

export type Edition = 'emergency' | 'full' | 'handover' | 'succession';
export type Paper = 'letter' | 'a4';
export type NoteAbout = 'property' | 'room' | 'item';

export interface BinderSection {
  key: string;
  volume: 'main' | 'sealed';
  short: string;
  title: string;
  changed: boolean;
  last_printed: DateStr | null;
  fingerprint: string;
}

export interface EditionInfo {
  edition: Edition;
  label: string;
  description: string;
  has_sealed: boolean;
  last_printed: DateStr | null;
  sections: BinderSection[];
  changed: number;
}

export interface BinderInfo {
  property_name: string;
  editions: EditionInfo[];
  sealed_location: string | null;
  can_print_sealed: boolean;
  reminder: { task_id: string; active: boolean } | null;
  paper: Paper;
}

export interface NoteInfo {
  id: string;
  /** Null when sensitive and only the owner may see it. */
  text: string | null;
  sensitive: boolean;
  at: string;
}

export interface ContractorInfo {
  id?: string;
  name: string;
  trades: string[];
  phone?: string | null;
  email?: string | null;
  notes?: string | null;
  notes_private: boolean;
  emergency: boolean;
}

export interface BinderReceipt {
  property: string;
  edition: Edition;
  volume: 'main' | 'sealed';
  printed_on: DateStr;
  sections: Record<string, string>;
}

export interface BinderPdfArgs {
  edition: Edition;
  volume?: 'main' | 'sealed';
  sections?: string[];
  only_changed?: boolean;
  include_costs?: boolean;
  include_notes?: boolean;
  paper?: Paper;
  record?: boolean;
  /** Only while it is on screen at setup: printed on the sealed volume. */
  recovery_key?: string;
}

export type AboutKind = 'item' | 'completion' | 'fact' | 'contractor';

/** Commands that send a file's bytes too: [arguments, result]. */
export interface UploadCommands {
  attach: [
    { about_kind: AboutKind; about_id: string; kind?: AttachmentKind; name?: string; sensitive?: boolean },
    AttachmentInfo,
  ];
  scan_label: [None, { texts: string[]; items: ItemRow[] }];
  read_label: [None, LabelRead];
  restore_backup: [None, Status];
}

/** Commands that return bytes: their arguments. */
export interface DownloadCommands {
  attachment_file: { id: string; thumb?: boolean };
  full_backup: None;
  label_sheet_pdf: { item_ids?: string[]; preset?: string; layout?: SheetLayout };
  binder_pdf: BinderPdfArgs;
  /** The Typst source, photos and receipt for one PDF (the web typesets it in a separate module). */
  binder_bundle: BinderPdfArgs;
}

export const UPLOAD_COMMANDS: (keyof UploadCommands)[] = ['attach', 'scan_label', 'read_label', 'restore_backup'];
export const DOWNLOAD_COMMANDS: (keyof DownloadCommands)[] = [
  'attachment_file',
  'full_backup',
  'label_sheet_pdf',
  'binder_pdf',
  'binder_bundle',
];

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
  'climate_lookup',
  'profile',
  'onboard',
  'starting_plan',
  'all_suggested',
  'answer_suggestion',
  'attachments',
  'delete_attachment',
  'update_item',
  'add_field',
  'remove_field',
  'reveal_field',
  'add_details',
  'facts',
  'save_fact',
  'delete_fact',
  'reveal_fact',
  'costs',
  'set_currency',
  'label_sheet',
  'find_label',
  'binder',
  'set_sealed_location',
  'set_binder_reminder',
  'set_paper',
  'record_binder_print',
  'notes',
  'add_note',
  'delete_note',
  'contractors',
  'save_contractor',
  'delete_contractor',
  'sync_status',
  'sync_check_server',
  'sync_setup',
  'sync_confirm_recovery',
  'sync_now',
  'sync_devices',
  'sync_invite',
  'sync_invite_poll',
  'sync_invite_confirm',
  'sync_invite_cancel',
  'sync_join',
  'sync_join_poll',
  'sync_join_confirm',
  'sync_restore',
  'sync_remove_device',
  'sync_new_recovery_key',
  'sync_stop',
  'sync_dismiss_notices',
];
