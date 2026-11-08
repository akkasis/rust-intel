import { useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Copy, ExternalLink, Radar, RefreshCw, Star, Trash2, Users } from "lucide-react";
import { api } from "@/lib/api";
import type { Ban, Notice as NoticeT, PlayerProfile } from "@/lib/types";
import { PROVIDER_NAMES, fmtDate, fmtDateTime, fmtDuration, fmtRelative } from "@/lib/format";
import { openExternal } from "@/lib/external";
import { useTz } from "@/app/hooks";
import {
  Avatar,
  Badge,
  Button,
  Card,
  CardHeader,
  Dialog,
  EmptyState,
  ErrorState,
  Hint,
  LoadingBlock,
  Notice,
  ProvenanceChip,
  Segmented,
  Select,
  StateDot,
  Switch,
  Tabs,
} from "@/components/ui";
import { ActivityView } from "@/features/activity/ActivityView";
import { RiskPanel } from "@/features/bans/RiskPanel";
import { BattleMetricsCard } from "./BattleMetricsCard";

type Tab = "overview" | "activity" | "servers" | "bans" | "combat" | "connections";

export function ProfilePage() {
  const { steamId = "" } = useParams();
  const loc = useLocation();
  const [tab, setTab] = useState<Tab>("overview");
  const q = useQuery({ queryKey: ["profile", steamId], queryFn: () => api.profile(steamId) });
  const notices = (loc.state as { notices?: NoticeT[] } | null)?.notices ?? [];
  if (q.isLoading) return <LoadingBlock rows={8} />;
  if (q.error) return <ErrorState error={q.error} retry={() => q.refetch()} />;
  const p = q.data!;
  return (
    <div className="space-y-4">
      <ProfileHeader p={p} />
      {notices
        .filter((n) => n.level !== "info")
        .map((n, i) => (
          <Notice key={i} tone={n.level === "error" ? "error" : "warning"}>
            {n.text}
          </Notice>
        ))}
      <Tabs
        value={tab}
        onChange={setTab}
        tabs={[
          { value: "overview", label: "Обзор" },
          { value: "activity", label: "Активность" },
          { value: "servers", label: "Серверы", count: p.servers.length },
          { value: "bans", label: "Баны", count: p.bans.filter((b) => b.active).length },
          { value: "combat", label: "Бой" },
          { value: "connections", label: "Связи", count: p.connections.length },
        ]}
      />
      {tab === "overview" && <Overview p={p} />}
      {tab === "activity" && <ActivityView steamId={p.player.steam_id} />}
      {tab === "servers" && <Servers p={p} />}
      {tab === "bans" && <Bans p={p} />}
      {tab === "combat" && (
        <Card>
          <EmptyState
            title="Боевая статистика недоступна"
            text={
              <>
                {p.provenance.combat.note} Значения K/D, точности и хедшотов не синтезируются из рангов или часов Steam.
              </>
            }
          />
        </Card>
      )}
      {tab === "connections" && <Connections p={p} />}
    </div>
  );
}

function ProfileHeader({ p }: { p: PlayerProfile }) {
  const qc = useQueryClient();
  const nav = useNavigate();
  const tz = useTz();
  const pl = p.player;
  const [confirmDelete, setConfirmDelete] = useState(false);
  const inv = () => {
    qc.invalidateQueries({ queryKey: ["profile", pl.steam_id] });
    qc.invalidateQueries({ queryKey: ["dashboard"] });
    qc.invalidateQueries({ queryKey: ["players"] });
    qc.invalidateQueries({ queryKey: ["tracked"] });
  };
  const refresh = useMutation({
    mutationFn: () => api.refreshPlayer(pl.steam_id),
    onSuccess: (n) => {
      inv();
      const bad = n.filter((x) => x.level === "error" || x.level === "warning");
      if (bad.length) bad.forEach((x) => toast.warning(x.text));
      else toast.success("Профиль обновлён");
    },
    onError: (e) => toast.error(e.message),
  });
  const fav = useMutation({ mutationFn: () => api.setFavorite(pl.steam_id, !pl.favorite), onSuccess: inv });
  const track = useMutation({
    mutationFn: (enabled: boolean) => (p.tracked && !enabled ? api.removeTracking(pl.steam_id) : api.setTracking(pl.steam_id, enabled)),
    onSuccess: (_, enabled) => {
      inv();
      toast.success(enabled ? "Игрок добавлен в Live-трекер" : "Отслеживание выключено");
    },
    onError: (e) => toast.error(e.message),
  });
  const del = useMutation({
    mutationFn: () => api.deletePlayer(pl.steam_id),
    onSuccess: () => {
      inv();
      nav("/players");
      toast.success("Игрок и его история удалены из локальной базы");
    },
  });
  const copy = (s: string) => navigator.clipboard?.writeText(s).then(() => toast.success("Скопировано"));
  return (
    <Card className="p-4">
      <div className="flex flex-wrap items-start gap-4">
        <Avatar name={pl.persona_name} url={pl.avatar_url} size={72} demo={pl.is_demo} />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h1 className="truncate font-display text-xl font-semibold">{pl.persona_name ?? "Имя неизвестно"}</h1>
            {pl.is_demo && <Badge tone="violet">вымышленный</Badge>}
            {pl.steam_in_game === true && <Badge tone="lime">в Rust по данным Steam</Badge>}
            <Badge tone={pl.visibility === "public" ? "teal" : "amber"}>
              {{ public: "профиль открыт", private: "профиль закрыт", friends_only: "только друзья" }[pl.visibility] ?? "видимость неизвестна"}
            </Badge>
            {p.bans.some((b) => b.active && b.kind !== "project") && <Badge tone="danger">есть баны Steam</Badge>}
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted">
            <button type="button" className="flex items-center gap-1 hover:text-fg num" onClick={() => copy(pl.steam_id)} title="Скопировать SteamID64">
              {pl.steam_id} <Copy className="size-3" />
            </button>
            <span className="num">{p.steam2}</span>
            <span className="num">{p.steam3}</span>
            {pl.profile_url && (
              <button type="button" className="flex items-center gap-1 text-teal hover:underline" onClick={() => openExternal(pl.profile_url!)}>
                Steam <ExternalLink className="size-3" />
              </button>
            )}
            {p.links
              .filter((l) => l.source_url)
              .map((l) => (
                <button key={l.provider} type="button" className="flex items-center gap-1 text-teal hover:underline" onClick={() => openExternal(l.source_url!)} title={`Привязка: ${l.match_evidence}, уверенность ${l.confidence}`}>
                  {PROVIDER_NAMES[l.provider] ?? l.provider} <ExternalLink className="size-3" />
                </button>
              ))}
          </div>
          <div className="mt-2">
            <ProvenanceChip p={p.provenance.profile} />
          </div>
        </div>
        <div className="flex flex-col items-end gap-2">
          <div className="flex gap-1.5">
            <Button size="icon" variant="ghost" onClick={() => fav.mutate()} aria-label={pl.favorite ? "Убрать из избранного" : "В избранное"} title="Избранное">
              <Star className={pl.favorite ? "size-4 fill-amber text-amber" : "size-4"} />
            </Button>
            <Button onClick={() => refresh.mutate()} loading={refresh.isPending} disabled={pl.is_demo} title={pl.is_demo ? "Демо-профили не обновляются из сети" : "Запросить свежие данные у источников"}>
              <RefreshCw className="size-3.5" /> Обновить
            </Button>
            <Button size="icon" variant="ghost" onClick={() => setConfirmDelete(true)} aria-label="Удалить игрока из базы" title="Удалить из локальной базы">
              <Trash2 className="size-4" />
            </Button>
          </div>
          <label className="flex items-center gap-2 text-xs text-muted">
            <Radar className="size-3.5" />
            Live-трекер
            <Switch checked={!!p.tracked?.enabled} onChange={(v) => track.mutate(v)} label="Отслеживать игрока" />
          </label>
          {p.last_seen && (
            <div className="flex items-center gap-1.5 text-[11px] text-muted" title={p.last_seen.approximate ? "Время приблизительное (по опросам)" : undefined}>
              <StateDot state={p.last_seen.open ? "online" : "offline"} />
              {p.last_seen.open ? "сейчас на " : "последний раз: "}
              {p.last_seen.scope === "observed_server" ? (p.last_seen.server_name ?? p.last_seen.server_id) : "Steam: в Rust"}
              {!p.last_seen.open && <span className="num">· {fmtDateTime(p.last_seen.at, tz)}</span>}
            </div>
          )}
        </div>
      </div>
      <Dialog
        open={confirmDelete}
        onClose={() => setConfirmDelete(false)}
        title="Удалить игрока?"
        footer={
          <>
            <Button onClick={() => setConfirmDelete(false)}>Отмена</Button>
            <Button variant="danger" loading={del.isPending} onClick={() => del.mutate()}>
              Удалить
            </Button>
          </>
        }
      >
        <p className="text-[13px] text-muted">Профиль, наблюдения, сессии, события и членство в отрядах будут удалены из локальной базы. Это действие необратимо.</p>
      </Dialog>
    </Card>
  );
}

function Overview({ p }: { p: PlayerProfile }) {
  const tz = useTz();
  const pl = p.player;
  const qc = useQueryClient();
  const [notes, setNotes] = useState(pl.notes ?? "");
  const saveNotes = useMutation({ mutationFn: () => api.setNotes(pl.steam_id, notes || null), onSuccess: () => (qc.invalidateQueries({ queryKey: ["profile", pl.steam_id] }), toast.success("Заметка сохранена")) });
  const ageDays = pl.account_created_at ? Math.floor((Date.now() - new Date(pl.account_created_at).getTime()) / 86_400_000) : null;
  return (
    <div className="grid gap-4 xl:grid-cols-[1.2fr_1fr]">
      <div className="space-y-4">
        <Card>
          <CardHeader title="Аккаунт Steam" actions={<ProvenanceChip p={p.provenance.profile} compact />} />
          <dl className="grid grid-cols-2 gap-x-4 gap-y-3 p-4 text-[13px] sm:grid-cols-3">
            <Fact label="Создан" value={pl.account_created_at ? `${fmtDate(pl.account_created_at, tz)} (${ageDays} дн.)` : "источник не сообщает"} />
            <Fact label="Часы в Rust" value={pl.rust_hours !== null ? `${Math.round(pl.rust_hours).toLocaleString("ru-RU")} ч` : pl.rust_hours_status === "hidden" ? "скрыты настройками приватности" : "нет данных"} extra={<ProvenanceChip p={p.provenance.hours} compact />} />
            <Fact label="Страна" value={pl.country_code && pl.country_code !== "ZZ" ? pl.country_code : "не указана"} />
            <Fact label="Наблюдаемых сессий" value={String(p.sessions_total)} extra={<Hint text={p.provenance.sessions.note ?? ""} />} />
            <Fact label="Первая наблюдаемая" value={fmtDateTime(p.first_session_at, tz)} />
            <Fact label="Последняя наблюдаемая" value={fmtDateTime(p.last_session_at, tz)} />
          </dl>
        </Card>
        <Card>
          <CardHeader title="Последние сессии" subtitle="С наблюдаемых источников" />
          {p.recent_sessions.length === 0 ? (
            <EmptyState title="Сессий нет" text="Включите Live-трекер или подключите источник с историей сессий." />
          ) : (
            <ul className="divide-y divide-line">
              {p.recent_sessions.slice(0, 8).map((s) => (
                <li key={s.id} className="flex items-center justify-between gap-3 px-4 py-1.5 text-[13px]">
                  <span className="truncate">{s.server_name ?? s.server_id ?? "Steam: в Rust"}</span>
                  <span className="shrink-0 text-xs text-muted num">
                    {fmtDateTime(s.start_upper, tz)} · ≥ {fmtDuration((new Date(s.end_lower).getTime() - new Date(s.start_upper).getTime()) / 1000)}
                    {s.is_open && <Badge tone="lime" className="ml-1.5">идёт</Badge>}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>
      <div className="space-y-4">
        <BattleMetricsCard p={p} />
        <Card>
          <CardHeader title="История ников" subtitle="С датами и источником" />
          {p.aliases.length === 0 ? (
            <EmptyState title="Нет данных" />
          ) : (
            <ul className="divide-y divide-line">
              {p.aliases.map((a) => (
                <li key={a.name + a.source} className="flex items-center justify-between gap-2 px-4 py-1.5 text-[13px]">
                  <span className="truncate">{a.name}</span>
                  <span className="shrink-0 text-[11px] text-faint">
                    {fmtDate(a.first_seen_at, tz)} – {fmtDate(a.last_seen_at, tz)} · {PROVIDER_NAMES[a.source] ?? a.source}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Card>
        <RiskPanel risk={p.risk} />
        <Card>
          <CardHeader title="Отряды" icon={<Users className="size-4" />} />
          <div className="flex flex-wrap gap-2 p-3">
            {p.squads.length === 0 && <span className="text-xs text-muted">Не состоит в отрядах</span>}
            {p.squads.map((s) => (
              <Link key={s.id} to={`/squads/${s.id}`} className="flex items-center gap-1.5 rounded border border-line px-2 py-1 text-xs hover:border-line-strong">
                <span className="size-2 rounded-full" style={{ background: s.color }} />
                {s.name}
              </Link>
            ))}
            <AddToSquad steamId={pl.steam_id} />
          </div>
        </Card>
        <Card>
          <CardHeader title="Заметка" subtitle="Только локально" />
          <div className="space-y-2 p-3">
            <textarea value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={2000} rows={3} className="w-full rounded-md border border-line bg-panel-2 p-2 text-[13px] focus:border-teal/60 focus:outline-none" aria-label="Заметка" />
            <Button size="sm" onClick={() => saveNotes.mutate()} loading={saveNotes.isPending}>
              Сохранить
            </Button>
          </div>
        </Card>
      </div>
    </div>
  );
}

function AddToSquad({ steamId }: { steamId: string }) {
  const qc = useQueryClient();
  const squads = useQuery({ queryKey: ["squads"], queryFn: api.squads });
  const [sel, setSel] = useState("");
  const add = useMutation({
    mutationFn: (id: number) => api.addSquadMember(id, steamId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["profile", steamId] });
      qc.invalidateQueries({ queryKey: ["squads"] });
      toast.success("Добавлен в отряд");
      setSel("");
    },
    onError: (e) => toast.error(e.message),
  });
  if (!squads.data?.length) return null;
  return (
    <Select value={sel} onChange={(e) => (setSel(e.target.value), e.target.value && add.mutate(Number(e.target.value)))} aria-label="Добавить в отряд" className="h-7 text-xs">
      <option value="">+ в отряд…</option>
      {squads.data.map((s) => (
        <option key={s.id} value={s.id}>
          {s.name}
        </option>
      ))}
    </Select>
  );
}

function Fact({ label, value, extra }: { label: string; value: string; extra?: React.ReactNode }) {
  return (
    <div className="min-w-0">
      <dt className="text-[11px] text-faint">{label}</dt>
      <dd className="flex items-center gap-1.5 truncate num">
        {value} {extra}
      </dd>
    </div>
  );
}

function Servers({ p }: { p: PlayerProfile }) {
  const tz = useTz();
  return (
    <div className="space-y-4">
      {p.server_stats.length > 0 && (
        <Card>
          <CardHeader title="Серверы по данным BattleMetrics" subtitle="Где игрок играл, сколько времени и когда последний раз" />
          <table className="w-full text-[13px]">
            <thead className="text-left text-[11px] tracking-wide text-faint uppercase">
              <tr>
                <th className="px-4 py-2 font-medium">Сервер</th>
                <th className="px-2 py-2 font-medium">Сейчас</th>
                <th className="px-2 py-2 text-right font-medium">Наиграно</th>
                <th className="px-2 py-2 font-medium">Первый раз</th>
                <th className="px-4 py-2 font-medium">Последний раз</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {p.server_stats.map((s) => (
                <tr key={s.server_id} className="hover:bg-raised">
                  <td className="max-w-[320px] truncate px-4 py-2">{s.server_name ?? s.server_id}</td>
                  <td className="px-2 py-2">{s.online ? <Badge tone="lime">онлайн</Badge> : <span className="text-faint">—</span>}</td>
                  <td className="px-2 py-2 text-right num">{s.time_played_secs !== null ? fmtDuration(s.time_played_secs) : "—"}</td>
                  <td className="px-2 py-2 text-muted num">{fmtDateTime(s.first_seen, tz)}</td>
                  <td className="px-4 py-2 text-muted num">{fmtDateTime(s.last_seen, tz)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="border-t border-line px-4 py-2 text-[11px] text-faint">Обновлено {fmtRelative(p.server_stats[0]!.fetched_at)} · кнопка «Загрузить данные» в блоке BattleMetrics на вкладке «Обзор»</div>
        </Card>
      )}
    <Card>
      <CardHeader title="Наблюдаемые серверы" subtitle="Только серверы, где источник видел игрока. Это не полный список." />
      {p.servers.length === 0 ? (
        <EmptyState title="Нет наблюдаемых сессий на серверах" text="Steam сообщает лишь «в Rust», без сервера. Привяжите игрока BattleMetrics (вкладка «Обзор») — появятся его серверы и сессии." />
      ) : (
        <table className="w-full text-[13px]">
          <thead className="text-left text-[11px] tracking-wide text-faint uppercase">
            <tr>
              <th className="px-4 py-2 font-medium">Сервер</th>
              <th className="px-2 py-2 text-right font-medium">Сессий</th>
              <th className="px-2 py-2 text-right font-medium">Наблюдалось онлайн</th>
              <th className="px-4 py-2 font-medium">Последний раз</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {p.servers.map((s) => (
              <tr key={s.server_id} className="hover:bg-raised">
                <td className="px-4 py-2">{s.server_name ?? s.server_id}</td>
                <td className="px-2 py-2 text-right num">{s.sessions}</td>
                <td className="px-2 py-2 text-right num">{fmtDuration(s.online_secs)}</td>
                <td className="px-4 py-2 text-muted num">{fmtDateTime(s.last_at, tz)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Card>
    </div>
  );
}

const BAN_LABEL: Record<Ban["kind"], string> = { vac: "VAC", game: "Game Ban", community: "Community Ban", economy: "Trade/Economy Ban", project: "Бан проекта" };

export function BanList({ bans, tz }: { bans: Ban[]; tz: string }) {
  if (bans.length === 0) return <EmptyState title="Нет данных о банах" text="Steam Web API не настроен или профиль ещё не обновлялся." />;
  return (
    <ul className="divide-y divide-line">
      {bans.map((b) => (
        <li key={b.kind + b.project_name + b.provider} className="flex items-start justify-between gap-3 px-4 py-2 text-[13px]">
          <div>
            <div className="flex items-center gap-2">
              <span className="font-medium">{BAN_LABEL[b.kind]}</span>
              {b.project_name && <span className="text-muted">· {b.project_name}</span>}
              <Badge tone={b.active ? "danger" : "neutral"}>{b.active ? (b.count ? `есть · ${b.count}` : "есть") : "нет"}</Badge>
              {b.revoked === true && <Badge tone="teal">отменён</Badge>}
            </div>
            <div className="text-[11px] text-faint">
              {b.kind === "game" && "Steam не указывает игру — это не обязательно бан Rust/EAC. "}
              {b.days_since_last !== null && b.active && `последний ${b.days_since_last} дн. назад · `}
              {b.reason && `причина: ${b.reason} · `}
              источник: {PROVIDER_NAMES[b.provider] ?? b.provider} · проверено {fmtDateTime(b.observed_at, tz)}
            </div>
          </div>
          {b.source_url && (
            <Button size="iconSm" variant="ghost" onClick={() => openExternal(b.source_url!)} aria-label="Открыть источник">
              <ExternalLink className="size-3.5" />
            </Button>
          )}
        </li>
      ))}
    </ul>
  );
}

function Bans({ p }: { p: PlayerProfile }) {
  const tz = useTz();
  const steam = p.bans.filter((b) => b.kind !== "project");
  const project = p.bans.filter((b) => b.kind === "project");
  return (
    <div className="grid gap-4 xl:grid-cols-2">
      <Card>
        <CardHeader title="Steam" subtitle="VAC / Game / Community / Economy — раздельно" actions={<ProvenanceChip p={p.provenance.bans} compact />} />
        <BanList bans={steam} tz={tz} />
      </Card>
      <div className="space-y-4">
        <Card>
          <CardHeader title="Баны проектов" subtitle="Только из документированных источников" />
          {project.length ? <BanList bans={project} tz={tz} /> : <EmptyState title="Источник не подключён" text="Нет подтверждённых документированных API банов проектов. Произвольные URL не принимаются." />}
        </Card>
        <RiskPanel risk={p.risk} />
      </div>
    </div>
  );
}

const CONF_LABEL: Record<string, { label: string; tone: "teal" | "amber" | "neutral" }> = {
  high: { label: "высокая", tone: "teal" },
  medium: { label: "средняя", tone: "amber" },
  low: { label: "низкая", tone: "neutral" },
  unknown: { label: "—", tone: "neutral" },
};

function Connections({ p }: { p: PlayerProfile }) {
  const tz = useTz();
  const [filter, setFilter] = useState<"together" | "friends" | "all">("together");
  const list = p.connections.filter((c) => (filter === "together" ? c.kind !== "friend" : filter === "friends" ? c.steam_friend : true));
  const friendsTotal = p.connections.filter((c) => c.steam_friend).length;
  const hasBm = p.links.some((l) => l.provider === "battlemetrics") || p.player.is_demo;
  return (
    <Card>
      <CardHeader
        title="Тиммейты и друзья"
        subtitle="Тиммейты — повторяющиеся совместные входы/выходы на одном сервере (все игроки BattleMetrics). Друг Steam, который играет вместе, — самый сильный сигнал."
        actions={
          <Segmented
            label="Показать"
            value={filter}
            onChange={setFilter}
            options={[
              { value: "together", label: "Играют вместе" },
              { value: "friends", label: `Друзья Steam (${friendsTotal})` },
              { value: "all", label: "Все" },
            ]}
          />
        }
      />
      {!hasBm && <Notice tone="warning" className="m-3">Игрок не связан с BattleMetrics — совместная игра не видна. Свяжите в блоке BattleMetrics на вкладке «Обзор».</Notice>}
      {p.player.friends_status === "private" && <Notice tone="info" className="m-3">Список друзей Steam скрыт игроком — показаны только тиммейты по совместной игре.</Notice>}
      {list.length === 0 ? (
        <EmptyState
          title={filter === "friends" ? "Друзей не найдено" : "Тиммейты пока не найдены"}
          text="Тиммейты появляются, когда программа наблюдает серверы игрока и видит повторяющиеся совместные входы/выходы."
        />
      ) : (
        <div className="max-h-[560px] overflow-y-auto">
          <table className="w-full text-[13px]">
            <thead className="sticky top-0 bg-panel text-left text-[11px] tracking-wide text-faint uppercase">
              <tr>
                <th className="px-4 py-2 font-medium">Игрок</th>
                <th className="px-2 py-2 font-medium">Связь</th>
                <th className="px-2 py-2 text-right font-medium">Вошли / вышли вместе</th>
                <th className="px-2 py-2 text-right font-medium">Вместе онлайн</th>
                <th className="px-2 py-2 font-medium">Последний раз</th>
                <th className="px-4 py-2 font-medium">Уверенность</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {list.map((c, i) => (
                <tr key={(c.bm_id ?? "") + (c.steam_id ?? "") + i} className="align-top hover:bg-raised">
                  <td className="px-4 py-2">
                    {c.bm_id ? (
                      <Link to={`/bm/${c.bm_id}`} className="font-medium hover:text-teal">
                        {c.name ?? c.bm_id}
                      </Link>
                    ) : c.steam_id ? (
                      <Link to={`/search?q=${c.steam_id}`} className="font-medium hover:text-teal" title="Открыть профиль Steam">
                        {c.name ?? c.steam_id}
                      </Link>
                    ) : (
                      <span>{c.name}</span>
                    )}
                    <div className="text-[11px] text-faint">{c.reasons.join(" · ")}</div>
                  </td>
                  <td className="px-2 py-2">
                    <div className="flex flex-wrap gap-1">
                      {c.steam_friend && <Badge tone="teal">друг Steam</Badge>}
                      {c.kind !== "friend" && <Badge tone="lime">играют вместе</Badge>}
                      {c.in_rust_now && <Badge tone="lime">сейчас в Rust</Badge>}
                      {c.in_same_squad && <Badge>в одном отряде</Badge>}
                    </div>
                  </td>
                  <td className="px-2 py-2 text-right num">{c.kind === "friend" ? "—" : `${c.joined_together} / ${c.left_together}`}</td>
                  <td className="px-2 py-2 text-right num">{c.overlap_secs ? fmtDuration(c.overlap_secs) : "—"}</td>
                  <td className="px-2 py-2 text-muted num">{c.last_together ? fmtRelative(c.last_together) : c.friend_since ? `дружат с ${fmtDateTime(c.friend_since, tz, { hour: undefined, minute: undefined })}` : "—"}</td>
                  <td className="px-4 py-2">
                    <Badge tone={CONF_LABEL[c.confidence]?.tone ?? "neutral"}>{CONF_LABEL[c.confidence]?.label ?? c.confidence}</Badge>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <div className="border-t border-line px-4 py-2 text-[11px] text-faint">Тиммейт — гипотеза по совпадению времени. Друзья Steam берутся из открытого списка друзей.</div>
    </Card>
  );
}
