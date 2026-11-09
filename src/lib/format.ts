// Форматирование: даты в выбранном IANA-поясе, длительности, русские склонения.

export function plural(n: number, one: string, few: string, many: string): string {
  const a = Math.abs(n) % 100;
  const b = a % 10;
  if (a > 10 && a < 20) return many;
  if (b > 1 && b < 5) return few;
  if (b === 1) return one;
  return many;
}

export function fmtDuration(secs: number): string {
  if (!Number.isFinite(secs) || secs <= 0) return "0 мин";
  const h = Math.floor(secs / 3600);
  const m = Math.round((secs % 3600) / 60);
  if (h >= 48) return `${h} ч`;
  if (h > 0) return m ? `${h} ч ${m} мин` : `${h} ч`;
  return `${m} мин`;
}

export function fmtHours(secs: number): string {
  return `${(secs / 3600).toFixed(secs < 36000 ? 1 : 0)} ч`;
}

export function fmtPct(x: number | null | undefined, digits = 0): string {
  if (x === null || x === undefined || !Number.isFinite(x)) return "—";
  return `${(x * 100).toFixed(digits)}%`;
}

export function fmtDateTime(iso: string | null | undefined, tz: string, opts: Intl.DateTimeFormatOptions = {}): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  try {
    return new Intl.DateTimeFormat("ru-RU", { timeZone: tz, day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit", ...opts }).format(d);
  } catch {
    return d.toISOString();
  }
}

export function fmtDate(iso: string | null | undefined, tz: string): string {
  return fmtDateTime(iso, tz, { hour: undefined, minute: undefined, year: "numeric" });
}

export function fmtTime(iso: string | null | undefined, tz: string): string {
  return fmtDateTime(iso, tz, { day: undefined, month: undefined });
}

/** Интервал неопределённости для событий/сессий из опросов: «≈ 10:00–10:05». */
export function fmtApprox(lower: string, upper: string | null, tz: string): string {
  if (!upper || lower === upper) return fmtDateTime(lower, tz);
  const a = fmtDateTime(lower, tz);
  const b = fmtTime(upper, tz);
  return `≈ ${a}–${b}`;
}

export function fmtRelative(iso: string | null | undefined, now: Date = new Date()): string {
  if (!iso) return "никогда";
  const d = new Date(iso).getTime();
  if (Number.isNaN(d)) return "—";
  const s = Math.round((now.getTime() - d) / 1000);
  if (s < 0) return "в будущем";
  if (s < 60) return "только что";
  const m = Math.round(s / 60);
  if (m < 60) return `${m} ${plural(m, "минуту", "минуты", "минут")} назад`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h} ${plural(h, "час", "часа", "часов")} назад`;
  const dd = Math.round(h / 24);
  return `${dd} ${plural(dd, "день", "дня", "дней")} назад`;
}

export const WEEKDAYS = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];

export function initials(name: string | null | undefined): string {
  const n = (name ?? "?").replace(/^DEMO·/, "").trim();
  return (n.slice(0, 2) || "?").toUpperCase();
}

export function shortServer(id: string | null | undefined, name?: string | null): string {
  return name ?? id ?? "—";
}

export function browserTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  } catch {
    return "UTC";
  }
}

export const EVENT_TITLES: Record<string, string> = {
  first_seen: "Впервые замечен",
  joined_observed_server: "Зашёл на наблюдаемый сервер",
  left_observed_server: "Покинул наблюдаемый сервер",
  switched_observed_server: "Сменил сервер",
  steam_in_game_started: "Запустил Rust (Steam)",
  steam_in_game_stopped: "Вышел из Rust (Steam)",
  name_changed: "Сменил ник",
  ban_change: "Изменение банов",
  source_unavailable: "Источник недоступен",
  source_restored: "Источник восстановлен",
  clan_gathering: "Клан заходит",
  teammate_died: "Тиммейт погиб",
};

export const PROVIDER_NAMES: Record<string, string> = {
  steam: "Steam Web API",
  battlemetrics: "BattleMetrics.com",
  battlemetrics_ru: "battlemetrics.ru",
  demo: "Демо (вымышленные)",
  telegram: "Telegram",
  project_bans: "Баны проектов",
  local_cache: "Локальная база",
};
