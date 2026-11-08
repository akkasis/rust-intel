// Типы, зеркалирующие DTO Rust-ядра (snake_case). SteamID64 — всегда string.

export type SteamId = string;
export type Iso = string;
export type Mode = "demo" | "live";
export type DataStatus = "verified" | "stale" | "partial" | "unsupported" | "not_configured" | "error" | "demo";

export interface Provenance {
  source: string;
  fetched_at: Iso | null;
  coverage: number | null;
  status: DataStatus;
  source_url: string | null;
  note: string | null;
}

export interface Player {
  steam_id: SteamId;
  persona_name: string | null;
  avatar_url: string | null;
  profile_url: string | null;
  visibility: "public" | "private" | "friends_only" | "unknown" | string;
  account_created_at: Iso | null;
  country_code: string | null;
  rust_hours: number | null;
  rust_hours_status: string;
  steam_in_game: boolean | null;
  steam_status: string;
  steam_fetched_at: Iso | null;
  is_demo: boolean;
  favorite: boolean;
  notes: string | null;
  created_at: Iso;
  updated_at: Iso;
}

export interface Alias {
  name: string;
  source: string;
  first_seen_at: Iso;
  last_seen_at: Iso;
}

export interface Ban {
  steam_id: SteamId;
  provider: string;
  kind: "vac" | "game" | "community" | "economy" | "project";
  project_name: string;
  count: number | null;
  active: boolean;
  days_since_last: number | null;
  reason: string | null;
  revoked: boolean | null;
  source_url: string | null;
  observed_at: Iso;
}

export interface ProviderLink {
  steam_id: SteamId;
  provider: string;
  external_id: string;
  match_evidence: string;
  confidence: string;
  source_url: string | null;
  linked_at: Iso;
}

export interface Session {
  id: number;
  steam_id: SteamId;
  provider: string;
  kind: "inferred_from_polls" | "provider_reported";
  server_id: string | null;
  server_name: string | null;
  start_lower: Iso;
  start_upper: Iso;
  end_lower: Iso;
  end_upper: Iso | null;
  is_open: boolean;
  external_id: string | null;
}

export interface Server {
  id: string;
  provider: string;
  external_id: string;
  name: string;
  address: string | null;
  country: string | null;
  map: string | null;
  players: number | null;
  max_players: number | null;
  queue: number | null;
  online: boolean | null;
  trust: string | null;
  last_wipe_at: Iso | null;
  next_wipe_at: Iso | null;
  followed: boolean;
  source_url: string | null;
  fetched_at: Iso;
  is_demo: boolean;
}

export interface TrackedPlayer {
  steam_id: SteamId;
  enabled: boolean;
  notify: boolean;
  added_at: Iso;
  last_state: "online" | "offline" | "unknown" | null;
  last_server: string | null;
  last_polled_at: Iso | null;
  last_success_at: Iso | null;
  persona_name: string | null;
  avatar_url: string | null;
  is_demo: boolean;
}

export interface EventRow {
  id: number;
  steam_id: SteamId | null;
  persona_name: string | null;
  provider: string;
  kind: string;
  occurred_lower: Iso;
  occurred_upper: Iso;
  approximate: boolean;
  server_id: string | null;
  server_name: string | null;
  details: string;
  created_at: Iso;
  is_read: boolean;
  notified: boolean;
}

export interface Squad {
  id: number;
  name: string;
  color: string;
  notes: string | null;
  created_at: Iso;
  updated_at: Iso;
  member_count: number;
}

export interface SquadMember {
  steam_id: SteamId;
  persona_name: string | null;
  avatar_url: string | null;
  provenance: string;
  added_at: Iso;
  is_demo: boolean;
}

export interface Health {
  provider: string;
  status: string;
  last_success_at: Iso | null;
  last_error_at: Iso | null;
  last_error_kind: string | null;
  last_error: string | null;
  consecutive_failures: number;
  checked_at: Iso;
}

export interface RiskIndicator {
  id: string;
  label: string;
  weight: number;
  triggered: boolean;
  evidence: string;
  source: string;
}

export interface RiskSummary {
  enabled: boolean;
  score: number;
  max_score: number;
  indicators: RiskIndicator[];
  disclaimer: string;
}

export interface LastSeen {
  at: Iso;
  scope: "observed_server" | "steam_in_game";
  provider: string;
  server_id: string | null;
  server_name: string | null;
  open: boolean;
  approximate: boolean;
}

export interface ObservedServer {
  server_id: string;
  server_name: string | null;
  sessions: number;
  online_secs: number;
  last_at: Iso;
}

export interface ServerStat {
  server_id: string;
  provider: string;
  server_name: string | null;
  online: boolean;
  first_seen: Iso | null;
  last_seen: Iso | null;
  time_played_secs: number | null;
  fetched_at: Iso;
}

export interface Connection {
  steam_id: SteamId;
  persona_name: string | null;
  co_sessions: number;
  co_online_secs: number;
  servers: string[];
  last_together: Iso;
  in_same_squad: boolean;
}

export interface PlayerProfile {
  player: Player;
  steam2: string;
  steam3: string;
  account_id: number;
  provenance: { profile: Provenance; bans: Provenance; hours: Provenance; sessions: Provenance; combat: Provenance };
  aliases: Alias[];
  bans: Ban[];
  links: ProviderLink[];
  sessions_total: number;
  first_session_at: Iso | null;
  last_session_at: Iso | null;
  last_seen: LastSeen | null;
  servers: ObservedServer[];
  server_stats: ServerStat[];
  recent_sessions: Session[];
  squads: Squad[];
  tracked: TrackedPlayer | null;
  risk: RiskSummary;
  connections: Connection[];
}

export type CellState = "known" | "insufficient" | "unknown";

export interface HeatCell {
  weekday: number;
  hour: number;
  online_secs: number;
  known_secs: number;
  sample_days: number;
  ratio: number | null;
  state: CellState;
}

export type TypicalOffline =
  | { status: "insufficient_data"; known_cells: number; required_cells: number }
  | { status: "available"; hours: [number, number][]; threshold: number };

export interface ActivityAggregate {
  timezone: string;
  range_start: Iso;
  range_end: Iso;
  range_secs: number;
  known_secs: number;
  online_secs: number;
  coverage: number;
  min_known_secs_per_cell: number;
  cells: HeatCell[];
  daily: { date: string; online_secs: number; known_secs: number }[];
  weekdays: { weekday: number; online_secs: number; known_secs: number; ratio: number | null }[];
  typical_offline: TypicalOffline;
}

export interface ActivityReport {
  steam_id: SteamId;
  aggregate: ActivityAggregate;
  sessions: Session[];
  sessions_truncated: boolean;
  sources: string[];
  servers: [string, string | null][];
  server_filter: string | null;
  poll_interval_secs: number | null;
  is_demo: boolean;
}

export type SquadState = "some_online" | "all_observed_offline" | "unknown";

export interface SquadSegment {
  start: Iso;
  end: Iso;
  online_mask: number;
  known_mask: number;
  online_count: number;
  state: SquadState;
}

export interface SquadHourCell {
  weekday: number;
  hour: number;
  some_online_secs: number;
  all_offline_secs: number;
  unknown_secs: number;
  all_offline_ratio: number | null;
  sample_days: number;
}

export interface SquadAnalysis {
  members: number;
  range_start: Iso;
  range_end: Iso;
  segments: SquadSegment[];
  hours: SquadHourCell[];
  pairs: { a: number; b: number; co_online_secs: number }[];
  concurrency_secs: number[];
  some_online_secs: number;
  all_offline_secs: number;
  unknown_secs: number;
  typical_offline: TypicalOffline;
}

export interface SquadReport {
  squad: Squad;
  members: SquadMember[];
  coverage: { steam_id: SteamId; known_secs: number; online_secs: number; coverage: number }[];
  analysis: SquadAnalysis | null;
  suggestions: {
    steam_id: SteamId;
    persona_name: string | null;
    co_sessions: number;
    with_members: string[];
    co_online_secs: number;
    evidence: { server_id: string; start: Iso; end: Iso; overlap_secs: number }[];
  }[];
  timezone: string;
}

export interface TrackerStatus {
  running: boolean;
  paused: boolean;
  last_cycle_at: Iso | null;
  next_cycle_at: Iso | null;
  interval_secs: number;
  sources: string[];
  note: string;
}

export interface Dashboard {
  mode: Mode;
  players_total: number;
  favorites: Player[];
  tracked: TrackedPlayer[];
  recent_events: EventRow[];
  unread_events: number;
  health: Health[];
  events_per_day: { date: string; count: number }[];
  squads: Squad[];
  followed_servers: Server[];
  tracker: TrackerStatus;
}

export interface Notice {
  level: "info" | "warning" | "error" | "hint";
  text: string;
}

export type ParsedInput =
  | { kind: "steam_id"; steam_id: SteamId; from_format: string }
  | { kind: "vanity"; vanity: string }
  | { kind: "ambiguous"; text: string };

export interface SearchOutcome {
  parsed: ParsedInput | null;
  resolved_steam_id: SteamId | null;
  candidates: {
    steam_id: SteamId | null;
    name: string;
    provider: string;
    external_id: string | null;
    source_url: string | null;
    last_seen: Iso | null;
    note: string;
    is_demo: boolean;
  }[];
  notices: Notice[];
}

export interface NotificationSettings {
  windows: boolean;
  telegram: boolean;
  telegram_chat_id: string | null;
  quiet_start: string | null;
  quiet_end: string | null;
  events: Record<string, boolean>;
}

export interface AppSettings {
  theme: "dark" | "light" | "system";
  language: "ru";
  timezone: string;
  autostart: boolean;
  close_to_tray: boolean;
  retention_days: number;
  poll_interval_secs: number;
  tracking_paused: boolean;
  risk_indicators: boolean;
  reduced_motion: "system" | "on" | "off";
  density: "compact" | "comfortable";
  notifications: NotificationSettings;
}

export interface AppInfo {
  version: string;
  mode: Mode | null;
  first_run: boolean;
  data_dir: string;
  settings: AppSettings;
  secret_backend: "os_keyring" | "memory_only";
  schema_version: number;
  platform: string;
}

export interface CapabilitySupport {
  capability: string;
  supported: boolean;
  requires: string;
  note: string;
}

export interface IntegrationStatus {
  descriptor: {
    id: string;
    name: string;
    kind: string;
    docs_url: string;
    credential: string | null;
    capabilities: CapabilitySupport[];
    terms_note: string;
  };
  configured: boolean;
  masked: string | null;
  health: Health | null;
  access: "verified" | "not_tested" | "denied" | "unsupported";
}

export type SecretKey = "steam_api_key" | "battlemetrics_token" | "battlemetrics_ru_key" | "telegram_bot_token";

export interface BanOverviewRow {
  player: Player;
  bans: Ban[];
  risk: RiskSummary;
}

export interface PopulationReport {
  server: Server;
  period: string;
  points: { t: Iso; players: number; source: string }[];
  provenance: Provenance;
}

export interface Diagnostics {
  integrity: string;
  schema_version: number;
  db_path: string;
  db_size_bytes: number;
  counts: [string, number][];
  health: Health[];
  tracker: TrackerStatus;
  secret_backend: string;
  generated_at: Iso;
}

export interface RangeSpec {
  preset: "24h" | "7d" | "30d" | "90d" | "custom";
  from?: Iso | null;
  to?: Iso | null;
}

export interface AppErrorPayload {
  kind: "validation" | "not_found" | "provider" | "db" | "first_run" | "io" | "internal";
  message: string;
  provider?: string;
  error?: { kind: string; retry_after_secs?: number | null };
}

// ---------------------------------------------------------------- BattleMetrics

export interface BmPlayer {
  bm_id: string;
  name: string | null;
  steam_id: SteamId | null;
  tracked: boolean;
  first_seen: Iso | null;
  last_seen: Iso | null;
  fetched_at: Iso | null;
  is_demo: boolean;
  updated_at: Iso;
}

export interface BmName {
  name: string;
  first_seen: Iso;
  last_seen: Iso;
}

export interface BmServer {
  server_id: string;
  server_name: string | null;
  online: boolean;
  first_seen: Iso | null;
  last_seen: Iso | null;
  time_played_secs: number | null;
  fetched_at: Iso;
}

export interface BmSession {
  id: number;
  bm_id: string;
  player_name: string | null;
  server_id: string;
  server_name: string | null;
  source: "server_watch" | "bm_api" | "demo";
  start_lower: Iso;
  start_upper: Iso;
  start_known: boolean;
  end_lower: Iso;
  end_upper: Iso | null;
  end_known: boolean;
  is_open: boolean;
}

export interface TogetherEvidence {
  server_id: string;
  start: Iso;
  end: Iso;
  overlap_secs: number;
  joined_together: boolean;
  left_together: boolean;
  join_delta_secs: number | null;
}

export interface Teammate {
  player: string;
  name: string | null;
  steam_id: SteamId | null;
  tracked: boolean;
  co_sessions: number;
  joined_together: number;
  left_together: number;
  overlap_secs: number;
  servers: string[];
  last_together: Iso;
  score: number;
  sync_rate: number;
  confidence: "high" | "medium" | "low";
  evidence: TogetherEvidence[];
}

export interface BmPlayerView {
  player: BmPlayer;
  source_url: string;
  names: BmName[];
  servers: BmServer[];
  sessions: BmSession[];
  sessions_total: number;
  observed_secs: number;
  online_now: BmServer | null;
  steam: Player | null;
  teammates: Teammate[];
  teammates_note: string;
  notices: Notice[];
  watched_servers: number;
}

export interface OnlinePlayer {
  bm_id: string;
  name: string;
  since: Iso | null;
  since_known: boolean;
  tracked: boolean;
  steam_id: SteamId | null;
}

export interface ServerOnline {
  server_id: string;
  fetched_at: Iso;
  source: string;
  watched: boolean;
  players: OnlinePlayer[];
  note: string | null;
}

export interface WatchRow {
  server_id: string;
  server_name: string | null;
  enabled: boolean;
  added_at: Iso;
  last_polled_at: Iso | null;
  last_success_at: Iso | null;
  last_error: string | null;
  last_online: number | null;
}
