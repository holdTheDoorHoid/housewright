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
  /** Floor plan units (D-06). */
  units: Units | null;
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
  /** Possible product recalls not yet looked at (0 when recall checks are off). */
  recall_matches: number;
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
  /** Who did it, when a contractor did (D-28). */
  contractor_id: string | null;
  contractor_name: string | null;
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
  /** Where a saved manual was downloaded from (D-151). */
  source: string | null;
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
  /** The page the advice comes from (manufacturer guidance, D-150). */
  source_link: string | null;
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
  /** The whole thing is marked private (D-41). */
  sensitive: boolean;
  /** Where it is on a floor plan (D-25). */
  pin: { pin_id: string; floor_id: string; floor_name: string; at: Point } | null;
  price: Money | null;
  value: Money | null;
  warranty: WarrantyInfo | null;
  /** The confirmed model family's links, facts and parts (D-149). */
  maker: MakerInfo | null;
  /** Families that fit the make and model, while none is confirmed. */
  family_suggestion: LibraryMatch | null;
}

// -- the appliance library (D-147 to D-152) --------------------------------

export type MatchConfidence = 'model' | 'possible' | 'brand';

export interface FamilyHit {
  id: string;
  name: string;
  brand_name: string;
  template: { id: string; name: string } | null;
  confidence: MatchConfidence;
  reasons: string[];
  pattern: string | null;
  corrected_model: string | null;
  unverified: boolean;
}

export interface LibraryMatch {
  brands: { id: string; name: string }[];
  families: FamilyHit[];
}

export interface MakerManual {
  kind: 'manual-search' | 'product-page' | 'support-page' | 'manual-pdf' | 'parts-page' | string;
  title: string;
  url: string;
  pdf: boolean;
  unverified: boolean;
}

export interface MakerFact {
  topic: string;
  text: string;
  source: string;
  checked: DateStr | null;
  unverified: boolean;
}

/** A filter, anode or other part (for "tools and supplies"). */
export interface Consumable {
  name: string;
  part: string | null;
  alternates: string[];
  size: string | null;
  replace_every: string | null;
  for_task: string | null;
  task_title: string | null;
  source: string;
  checked: DateStr | null;
  unverified: boolean;
}

export interface FamilyInfo {
  id: string;
  name: string;
  brand_id: string;
  brand_name: string;
  category: string;
  template: { id: string; name: string } | null;
  status: string;
  years: string | null;
  description: string | null;
  model_patterns: string[];
  model_examples: string[];
  unverified: boolean;
  note: string | null;
  website: string;
  support_url: string;
  manual_search_url: string | null;
  phone: string | null;
  manuals: MakerManual[];
  facts: MakerFact[];
  consumables: Consumable[];
  warranty: { summary: string; length: string | null; source: string; checked: DateStr | null; unverified: boolean } | null;
  recalls: { title: string; date: DateStr | null; url: string }[];
  credit: string;
}

export type Follows = 'maker' | 'generic';

export interface Guidance {
  task_id: string;
  task_title: string;
  key: string;
  generic: string;
  generic_source: string;
  maker: string;
  maker_title: string | null;
  maker_steps: string[] | null;
  note: string | null;
  source: string;
  checked: DateStr | null;
  unverified: boolean;
  following: Follows | null;
}

export interface MakerInfo {
  family_id: string;
  in_library: boolean;
  family: FamilyInfo | null;
  guidance: Guidance[];
  other_kind: boolean;
}

export interface LibraryStatus {
  version: string;
  commit: string;
  published: DateStr;
  source: string;
  licence: string;
  credit: string;
  brands: number;
  families: number;
  from_update: boolean;
  shipped_version: string;
  auto: boolean;
  checked_at: string | null;
  manifest_url: string;
  problems: number;
}

export interface LibraryCheck {
  newer: boolean;
  version: string;
  size: number;
  bundle_url: string | null;
  message: string;
}

export interface ManualSource {
  url: string;
  name: string;
  domains: string[];
}

export interface Supply {
  item_id: string;
  item_name: string;
  family_id: string;
  consumable: Consumable;
}

export interface Money {
  minor: number;
  currency: string;
}

export type WarrantyState = 'active' | 'ending_soon' | 'ended';

export interface WarrantyInfo {
  ends: DateStr;
  months: number | null;
  state: WarrantyState;
  days_left: number;
  provider: string | null;
  transferable: boolean;
  register_url: string | null;
  has_reference: boolean;
}

export interface WarrantyRow extends WarrantyInfo {
  item_id: string;
  name: string;
  room_name: string | null;
}

export interface Warranties {
  today: DateStr;
  ending_soon: WarrantyRow[];
  active: WarrantyRow[];
  ended: WarrantyRow[];
  without: number;
}

export interface WarrantyIn {
  remove?: boolean;
  months?: number;
  ends?: DateStr;
  provider?: string;
  register_url?: string;
  transferable?: boolean;
}

export type MatchLevel = 'model' | 'model_series' | 'brand_and_kind';

export interface RecallMatch {
  item_id: string;
  item_name: string;
  room_name: string | null;
  level: MatchLevel;
  evidence: string[];
  recall: {
    number: string;
    date: DateStr;
    title: string;
    url: string;
    hazard: string;
    remedy: string;
    products: string[];
  };
  dismissal_id: string | null;
}

export interface Recalls {
  enabled: boolean;
  synced_to: DateStr | null;
  checked_at: string | null;
  stored: number;
  matches: RecallMatch[];
  dismissed: RecallMatch[];
  /** What to download next: dates only. */
  next: { start: DateStr; end: DateStr; url: string }[];
  pause_ms: number;
}

export interface InsuranceInfo {
  property_name: string;
  things: number;
  with_photo: number;
  with_value: number;
  with_receipt: number;
  total_value: number;
  total_price: number;
  currency: string;
  sensitive_things: number;
  sensitive_details: number;
  can_include_sensitive: boolean;
  filename: string;
}

export interface InsuranceArgs {
  include_sensitive?: boolean;
  paper?: Paper;
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
  /** The task, or for a notice its stable key. */
  task_id: string;
  kind: 'due' | 'follow_up' | 'rescheduled' | 'notice';
  at_ms: number;
  title: string;
  body: string;
  /** A notice opens this (`item/<id>`, `contractor/<id>`). */
  open?: string;
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
  /** The floor plan pin it is linked to. */
  pin_id: string | null;
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
  /** Library families that fit the brand and model read (D-148). */
  family: LibraryMatch;
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
      /** An appliance library family the person accepted (D-149). */
      family?: string;
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
      /** Amounts as typed; "" clears. */
      price?: string;
      value?: string;
      warranty?: WarrantyIn;
      /** Owner only. */
      sensitive?: boolean;
      serial_sensitive?: boolean;
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
      /** A contractor's id, or "" for "not a contractor". */
      contractor_id?: string;
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
      /** Absent keeps the link; "" removes it. */
      pin_id?: string;
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
  save_contractor: [ContractorIn, ContractorInfo[]];
  delete_contractor: [{ id: string }, null];
  contractor: [{ id: string }, ContractorPage];
  warranties: [None, Warranties];
  recalls: [None, Recalls];
  set_recalls: [{ on: boolean }, Recalls];
  recall_import: [{ json: string; start: DateStr; end: DateStr }, { received: number; stored: number }];
  dismiss_recall: [{ item_id: string; number: string }, Recalls];
  undismiss_recall: [{ id: string }, Recalls];
  insurance: [None, InsuranceInfo];
  // -- floor plans (Phase 8) --
  floors: [None, FloorInfo[]];
  save_floor: [{ id?: string; name: string; level?: number }, FloorInfo[]];
  delete_floor: [{ id: string }, FloorInfo[]];
  floor_plan: [{ floor_id: string }, PlanInfo];
  save_room_shape: [{ floor_id: string; room_id?: string; name?: string; outline?: Point[] }, PlanSaved];
  remove_room_shape: [{ id: string }, PlanInfo];
  save_walls: [{ floor_id: string; walls: { id?: string; a: Point; b: Point; thickness?: number | null }[] }, PlanSaved];
  delete_wall: [{ id: string }, PlanInfo];
  save_opening: [
    { id?: string; wall_id: string; kind: OpeningKind; offset: number; width: number; swing?: Swing | null },
    PlanSaved,
  ];
  delete_opening: [{ id: string }, PlanInfo];
  save_pin: [
    { id?: string; floor_id: string; at: Point; item_id?: string; label?: string; wall_id?: string; sensitive?: boolean },
    PlanSaved,
  ];
  delete_pin: [{ id: string }, PlanInfo];
  save_label: [{ id?: string; floor_id: string; at: Point; text: string; sensitive?: boolean }, PlanSaved];
  delete_label: [{ id: string }, PlanInfo];
  set_background: [
    { floor_id: string; attachment_id?: string | null; at?: Point; width?: number; opacity?: number },
    PlanInfo,
  ];
  set_units: [{ units: Units }, Status];
  plan_restore: [{ floor_id: string; to: PlanState; from: PlanState }, { plan: PlanInfo; renamed: Record<string, string> }];
  // -- sync (Phase 6) --
  sync_status: [None, SyncStatus];
  sync_check_server: [{ server: string }, { ok: boolean; version: string; time_ms: number; registration_open: boolean }];
  sync_setup: [{ server: string; device_name: string; registration_token?: string }, RecoveryKeyOut];
  sync_confirm_recovery: [None, SyncStatus];
  sync_now: [None, { report: SyncReport; status: SyncStatus }];
  sync_devices: [None, SyncDevice[]];
  sync_invite: [{ who: 'device' | 'person' | 'home_assistant'; name?: string; role?: Role }, Invitation];
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
  library: [None, LibraryStatus];
  library_set_updates: [{ auto: boolean }, LibraryStatus];
  library_check: [{ manifest: string }, LibraryCheck];
  library_reset: [None, LibraryStatus];
  library_match: [{ make: string; model: string; template?: string | null }, LibraryMatch];
  library_families: [{ template?: string | null; brand?: string | null }, FamilyHit[]];
  library_family: [{ id: string }, FamilyInfo];
  /** A family id, 'none' (none of the suggestions fit), or null to clear. */
  set_family: [{ item_id: string; family: string | null }, ItemDetail];
  choose_guidance: [{ task_id: string; follow: Follows }, ItemDetail];
  manual_source: [{ item_id: string; url: string }, ManualSource];
  supplies: [None, Supply[]];
}

// -- floor plans (Phase 8, D-25) ---------------------------------------------

/** Plan millimetres; y points down. */
export interface Point {
  x: number;
  y: number;
}
export type Units = 'imperial' | 'metric';
export type OpeningKind = 'door' | 'window' | 'gap';
export interface Swing {
  hinge: 'a' | 'b';
  side: 'left' | 'right';
}

export interface FloorInfo {
  id: string;
  name: string;
  level: number;
  drawn: boolean;
  rooms: number;
  area_text: string | null;
}

export interface PlanRoom {
  id: string;
  name: string;
  outline: Point[] | null;
  area_mm2: number | null;
  area_text: string | null;
  perimeter_text: string | null;
  label_at: Point | null;
  things: number;
}

export interface PlanWall {
  id: string;
  a: Point;
  b: Point;
  thickness: number;
  length_mm: number;
  length_text: string;
}

export interface PlanOpening {
  id: string;
  wall_id: string;
  kind: OpeningKind;
  offset: number;
  width: number;
  width_text: string;
  swing: Swing | null;
}

export interface PlanPin {
  id: string;
  number: number;
  at: Point;
  item_id: string | null;
  item_name: string | null;
  label: string | null;
  wall_id: string | null;
  sensitive: boolean;
  room_name: string | null;
  fact_id: string | null;
  fact_label: string | null;
}

export interface PlanLabel {
  id: string;
  at: Point;
  text: string | null;
  sensitive: boolean;
}

/** The stored entities of one floor, kept by the editor for undo. */
export interface PlanState {
  walls: unknown[];
  openings: unknown[];
  pins: unknown[];
  labels: unknown[];
  rooms: unknown[];
}

export interface PlanInfo {
  floor: FloorInfo;
  floors: FloorInfo[];
  units: Units;
  rooms: PlanRoom[];
  walls: PlanWall[];
  openings: PlanOpening[];
  pins: PlanPin[];
  labels: PlanLabel[];
  spaces: { outline: Point[]; area_text: string; label_at: Point }[];
  total_area_text: string | null;
  background: {
    attachment_id: string;
    at: Point;
    width: number;
    opacity: number;
    pixel_width: number | null;
    pixel_height: number | null;
  } | null;
  corners: Point[];
  can_edit: boolean;
  state: PlanState;
}

export interface PlanSaved {
  plan: PlanInfo;
  ids: string[];
  moved_to: string | null;
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
  /** The Home Assistant bridge (D-117). */
  home_assistant: boolean;
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

export interface CredentialInfo {
  /** Null when marked sensitive and only the owner may see it. */
  number: string | null;
  expires: DateStr | null;
  state: 'ok' | 'ending_soon' | 'ended' | 'no_date';
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
  company?: string | null;
  website?: string | null;
  service_area?: string | null;
  licence?: CredentialInfo | null;
  insurance?: CredentialInfo | null;
  /** The household's own opinion: never printed or shared. Null for limited members. */
  private?: { rating: number | null; would_hire_again: boolean | null; notes: string | null } | null;
  jobs?: number;
  /** Minor units in the household's currency. */
  spent?: number;
}

/** What `save_contractor` takes. Absent new fields stay as they were; "" clears. */
export interface ContractorIn {
  id?: string;
  name: string;
  trades: string[];
  phone?: string | null;
  email?: string | null;
  notes?: string | null;
  notes_private: boolean;
  emergency: boolean;
  company?: string;
  website?: string;
  service_area?: string;
  licence_number?: string;
  licence_expires?: string;
  insurance_number?: string;
  insurance_expires?: string;
  /** 1 to 5, 0 clears. */
  private_rating?: number;
  would_hire_again?: 'yes' | 'no' | '';
  private_notes?: string;
}

export interface ContractorJob {
  completion_id: string;
  task_id: string;
  task_title: string;
  at: string;
  item_id: string | null;
  item_name: string | null;
  room_name: string | null;
  costs: { kind: CostKind; minor: number; currency: string }[];
  total: number;
}

export interface ContractorPage {
  contractor: ContractorInfo;
  jobs: ContractorJob[];
  currency: string;
  attachments: AttachmentInfo[];
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

export type AboutKind = 'item' | 'completion' | 'fact' | 'contractor' | 'floor';

/** Commands that send a file's bytes too: [arguments, result]. */
export interface UploadCommands {
  attach: [
    { about_kind: AboutKind; about_id: string; kind?: AttachmentKind; name?: string; sensitive?: boolean },
    AttachmentInfo,
  ];
  scan_label: [None, { texts: string[]; items: ItemRow[] }];
  read_label: [None, LabelRead];
  restore_backup: [None, Status];
  /** A downloaded library bundle, checked against its release note. */
  library_import: [{ manifest: string }, LibraryStatus];
  /** A manual PDF downloaded from one of the thing's family's links. */
  attach_manual: [{ item_id: string; url: string }, AttachmentInfo];
}

/** Commands that return bytes: their arguments. */
export interface DownloadCommands {
  attachment_file: { id: string; thumb?: boolean };
  full_backup: None;
  label_sheet_pdf: { item_ids?: string[]; preset?: string; layout?: SheetLayout };
  binder_pdf: BinderPdfArgs;
  /** The Typst source, photos and receipt for one PDF (the web typesets it in a separate module). */
  binder_bundle: BinderPdfArgs;
  /** The insurance inventory (Phase 9). */
  insurance_pdf: InsuranceArgs;
  insurance_bundle: InsuranceArgs;
  insurance_csv: InsuranceArgs;
}

export const UPLOAD_COMMANDS: (keyof UploadCommands)[] = [
  'attach',
  'scan_label',
  'read_label',
  'restore_backup',
  'library_import',
  'attach_manual',
];
export const DOWNLOAD_COMMANDS: (keyof DownloadCommands)[] = [
  'attachment_file',
  'full_backup',
  'label_sheet_pdf',
  'binder_pdf',
  'binder_bundle',
  'insurance_pdf',
  'insurance_bundle',
  'insurance_csv',
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
  'contractor',
  'warranties',
  'recalls',
  'set_recalls',
  'recall_import',
  'dismiss_recall',
  'undismiss_recall',
  'insurance',
  'floors',
  'save_floor',
  'delete_floor',
  'floor_plan',
  'save_room_shape',
  'remove_room_shape',
  'save_walls',
  'delete_wall',
  'save_opening',
  'delete_opening',
  'save_pin',
  'delete_pin',
  'save_label',
  'delete_label',
  'set_background',
  'set_units',
  'plan_restore',
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
  'library',
  'library_set_updates',
  'library_check',
  'library_reset',
  'library_match',
  'library_families',
  'library_family',
  'set_family',
  'choose_guidance',
  'manual_source',
  'supplies',
];
