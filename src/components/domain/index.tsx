// Доменные компоненты: строки событий, бейджи источников, карточки игроков.
import { Link } from "react-router";
import type { EventRow, Health, TrackedPlayer } from "@/lib/types";
import { EVENT_TITLES, PROVIDER_NAMES, fmtApprox, fmtRelative } from "@/lib/format";
import { Avatar, Badge, StateDot, cn } from "@/components/ui";
import { ArrowLeftRight, CircleAlert, CircleCheck, LogIn, LogOut, PenLine, ShieldAlert, Sparkles } from "lucide-react";

const EVENT_ICON: Record<string, typeof LogIn> = {
  joined_observed_server: LogIn,
  steam_in_game_started: LogIn,
  left_observed_server: LogOut,
  steam_in_game_stopped: LogOut,
  switched_observed_server: ArrowLeftRight,
  name_changed: PenLine,
  ban_change: ShieldAlert,
  source_unavailable: CircleAlert,
  source_restored: CircleCheck,
  first_seen: Sparkles,
};

const EVENT_TONE: Record<string, string> = {
  joined_observed_server: "text-lime",
  steam_in_game_started: "text-lime",
  left_observed_server: "text-muted",
  steam_in_game_stopped: "text-muted",
  switched_observed_server: "text-teal",
  name_changed: "text-violet",
  ban_change: "text-danger",
  source_unavailable: "text-amber",
  source_restored: "text-teal",
  first_seen: "text-teal",
};

function parseDetails(raw: string): Record<string, unknown> {
  try {
    const v: unknown = JSON.parse(raw);
    return v && typeof v === "object" ? (v as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

export function EventItem({ e, tz, compact }: { e: EventRow; tz: string; compact?: boolean }) {
  const Icon = EVENT_ICON[e.kind] ?? Sparkles;
  const details = parseDetails(e.details);
  return (
    <li className={cn("flex items-start gap-2.5 px-3 py-2", !e.is_read && "bg-teal/[0.03]")}>
      <Icon className={cn("mt-0.5 size-4 shrink-0", EVENT_TONE[e.kind] ?? "text-muted")} aria-hidden />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-baseline gap-x-1.5">
          {e.steam_id ? (
            <Link to={`/players/${e.steam_id}`} className="font-medium hover:text-teal">
              {e.persona_name ?? e.steam_id}
            </Link>
          ) : typeof details.clan_id === "number" ? (
            <Link to={`/clans/${details.clan_id}`} className="font-medium hover:text-teal">
              {String(details.name)}
            </Link>
          ) : typeof details.bm_id === "string" ? (
            <Link to={`/bm/${details.bm_id}`} className="font-medium hover:text-teal">
              {String(details.name ?? details.bm_id)}
            </Link>
          ) : (
            <span className="font-medium">{PROVIDER_NAMES[String(details.provider ?? e.provider)] ?? e.provider}</span>
          )}
          <span className="text-muted">{EVENT_TITLES[e.kind] ?? e.kind}</span>
          {e.kind === "teammate_died" && typeof details.name === "string" && (
            <span className="text-xs text-faint">
              {String(details.name)} · {typeof details.grid === "string" ? details.grid : "вне сетки"} · {String(details.server ?? "")}
            </span>
          )}
          {e.kind === "clan_gathering" && Array.isArray(details.members) && <span className="text-xs text-faint">{(details.members as string[]).join(", ")}</span>}
          {e.kind === "name_changed" && typeof details.from === "string" && (
            <span className="text-xs text-faint">
              {String(details.from)} → {String(details.to)}
            </span>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-x-2 text-[11px] text-faint">
          <span className="num" title={e.approximate ? "Время приблизительное: переход произошёл между двумя опросами" : "Точное время"}>
            {fmtApprox(e.occurred_lower, e.occurred_upper, tz)}
          </span>
          {(e.server_name || e.server_id) && !compact && <span className="truncate">· {e.server_name ?? e.server_id}</span>}
          {!compact && <span>· {PROVIDER_NAMES[e.provider] ?? e.provider}</span>}
          {typeof details.error === "string" && <span className="text-amber">· {details.error}</span>}
        </div>
      </div>
      {!e.is_read && <span className="mt-1.5 size-1.5 shrink-0 rounded-full bg-teal" aria-label="непрочитано" />}
    </li>
  );
}

export function HealthBadge({ h }: { h: Health | undefined }) {
  if (!h) return <Badge>не проверялся</Badge>;
  const tone = h.status === "ok" ? "teal" : h.status === "degraded" ? "amber" : h.status === "down" ? "danger" : "neutral";
  const label = { ok: "работает", degraded: "сбои", down: "недоступен", not_configured: "не настроен", unsupported: "не поддерживается" }[h.status] ?? h.status;
  return (
    <Badge tone={tone} title={h.last_error ?? `Последний успех: ${fmtRelative(h.last_success_at)}`}>
      {label}
    </Badge>
  );
}

export function TrackedRow({ t, tz }: { t: TrackedPlayer; tz: string }) {
  const stale = t.last_success_at && Date.now() - new Date(t.last_success_at).getTime() > 15 * 60_000;
  return (
    <Link to={`/players/${t.steam_id}`} className="flex items-center gap-2.5 rounded-md px-2 py-1.5 hover:bg-raised">
      <Avatar name={t.persona_name} url={t.avatar_url} size={26} demo={t.is_demo} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px] font-medium">{t.persona_name ?? t.steam_id}</div>
        <div className="truncate text-[11px] text-faint">
          {t.last_polled_at ? `опрос ${fmtRelative(t.last_polled_at)}` : "ещё не опрашивался"}
          {stale && <span className="text-amber"> · данные устарели</span>}
          {!t.enabled && " · пауза"}
        </div>
      </div>
      <StateDot state={t.last_state} />
      <span className="sr-only">{tz}</span>
    </Link>
  );
}
