import { Fragment, useMemo, useState } from "react";
import { Link, useParams } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ChevronDown, ChevronRight, ExternalLink, Link2, Radar, RefreshCw, UsersRound } from "lucide-react";
import { api } from "@/lib/api";
import type { BmPlayerView, BmSession, RangeSpec, Teammate } from "@/lib/types";
import { fmtApprox, fmtDateTime, fmtDuration, fmtPct, fmtRelative, fmtTime } from "@/lib/format";
import { openExternal } from "@/lib/external";
import { useIsDemo, useTz } from "@/app/hooks";
import { Avatar, Badge, Button, Card, CardHeader, EmptyState, ErrorState, Input, Kpi, LoadingBlock, Notice, Segmented, Select, Switch, Tabs, cn } from "@/components/ui";
import { ActivityHeatmap } from "@/components/charts/Heatmap";
import { TypicalOfflineCard } from "@/features/activity/ActivityView";

type Tab = "overview" | "teammates" | "sessions" | "activity";

const CONF: Record<Teammate["confidence"], { label: string; tone: "teal" | "amber" | "neutral" }> = {
  high: { label: "высокая", tone: "teal" },
  medium: { label: "средняя", tone: "amber" },
  low: { label: "низкая", tone: "neutral" },
};

export function BmPlayerPage() {
  const { bmId = "" } = useParams();
  const qc = useQueryClient();
  const [tab, setTab] = useState<Tab>("overview");
  const q = useQuery({ queryKey: ["bm-player", bmId], queryFn: () => api.bmPlayer(bmId) });
  const refresh = useMutation({
    mutationFn: () => api.bmPlayer(bmId, true),
    onSuccess: (v) => {
      qc.setQueryData(["bm-player", bmId], v);
      v.notices.forEach((n) => (n.level === "error" ? toast.error(n.text) : n.level === "warning" ? toast.warning(n.text) : toast.success(n.text)));
    },
    onError: (e) => toast.error(e.message),
  });
  const track = useMutation({
    mutationFn: (v: boolean) => api.setBmTracked(bmId, v),
    onSuccess: (_, v) => (qc.invalidateQueries({ queryKey: ["bm-player", bmId] }), toast.success(v ? "Игрок отслеживается на наблюдаемых серверах" : "Отслеживание выключено")),
  });
  if (q.isLoading) return <LoadingBlock rows={8} />;
  if (q.error) return <ErrorState error={q.error} retry={() => q.refetch()} />;
  const v = q.data!;
  const p = v.player;
  return (
    <div className="space-y-4">
      <Card className="p-4">
        <div className="flex flex-wrap items-start gap-4">
          <Avatar name={p.name} size={64} demo={p.is_demo} />
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <h1 className="truncate font-display text-xl font-semibold">{p.name ?? `Игрок BM ${p.bm_id}`}</h1>
              {p.is_demo && <Badge tone="violet">вымышленный</Badge>}
              {v.online_now ? <Badge tone="lime">онлайн · {v.online_now.server_name ?? v.online_now.server_id}</Badge> : <Badge>не онлайн на известных серверах</Badge>}
            </div>
            <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted">
              <span className="num">BattleMetrics ID {p.bm_id}</span>
              {!p.is_demo && (
                <button type="button" className="flex items-center gap-1 text-teal hover:underline" onClick={() => openExternal(v.source_url)}>
                  battlemetrics.com <ExternalLink className="size-3" />
                </button>
              )}
              {v.steam ? (
                <Link to={`/players/${v.steam.steam_id}`} className="text-teal hover:underline">
                  Steam: {v.steam.persona_name ?? v.steam.steam_id}
                </Link>
              ) : (
                <span>Steam не связан</span>
              )}
              <span>обновлено {fmtRelative(p.fetched_at)}</span>
            </div>
          </div>
          <div className="flex flex-col items-end gap-2">
            <Button onClick={() => refresh.mutate()} loading={refresh.isPending} disabled={p.is_demo} title={p.is_demo ? "Демо не обновляется из сети" : "Запросить BattleMetrics"}>
              <RefreshCw className="size-3.5" /> Обновить из BattleMetrics
            </Button>
            <label className="flex items-center gap-2 text-xs text-muted" title="Уведомления о входе/выходе на наблюдаемых серверах">
              <Radar className="size-3.5" /> Отслеживать
              <Switch checked={p.tracked} onChange={(x) => track.mutate(x)} label="Отслеживать игрока BM" />
            </label>
          </div>
        </div>
      </Card>
      {v.notices.filter((n) => n.level !== "info").map((n, i) => (
        <Notice key={i} tone={n.level === "error" ? "error" : "warning"}>
          {n.text}
        </Notice>
      ))}
      <Tabs
        value={tab}
        onChange={setTab}
        tabs={[
          { value: "overview", label: "Обзор" },
          { value: "teammates", label: "Тиммейты", count: v.teammates.length },
          { value: "sessions", label: "Сессии", count: v.sessions_total },
          { value: "activity", label: "Активность" },
        ]}
      />
      {tab === "overview" && <Overview v={v} />}
      {tab === "teammates" && <Teammates v={v} />}
      {tab === "sessions" && <Sessions sessions={v.sessions} total={v.sessions_total} />}
      {tab === "activity" && <BmActivity bmId={p.bm_id} />}
    </div>
  );
}

function Overview({ v }: { v: BmPlayerView }) {
  const tz = useTz();
  const played = v.servers.reduce((s, x) => s + (x.time_played_secs ?? 0), 0);
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
        <Kpi label="Серверов" value={v.servers.length} sub="по данным BattleMetrics" />
        <Kpi label="Наиграно (BM)" value={played ? fmtDuration(played) : "—"} sub="сумма по серверам" tone="lime" />
        <Kpi label="Сессий" value={v.sessions_total} sub={`наблюдалось ${fmtDuration(v.observed_secs)}`} />
        <Kpi label="Тиммейты" value={v.teammates.length} sub="гипотез" tone="teal" />
        <Kpi label="Последний раз" value={v.player.last_seen ? fmtRelative(v.player.last_seen) : "—"} sub={fmtDateTime(v.player.last_seen, tz)} />
      </div>
      <div className="grid gap-4 xl:grid-cols-[1.5fr_1fr]">
        <Card>
          <CardHeader title="Серверы" subtitle="Где играл, сколько и когда" />
          {v.servers.length === 0 ? (
            <EmptyState title="Нет данных о серверах" text="Нажмите «Обновить из BattleMetrics»." />
          ) : (
            <div className="max-h-[420px] overflow-y-auto">
              <table className="w-full text-[13px]">
                <thead className="sticky top-0 bg-panel text-left text-[11px] tracking-wide text-faint uppercase">
                  <tr>
                    <th className="px-4 py-2 font-medium">Сервер</th>
                    <th className="px-2 py-2 font-medium">Сейчас</th>
                    <th className="px-2 py-2 text-right font-medium">Наиграно</th>
                    <th className="px-2 py-2 font-medium">Первый раз</th>
                    <th className="px-4 py-2 font-medium">Последний раз</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-line">
                  {v.servers.map((s) => (
                    <tr key={s.server_id} className="hover:bg-raised">
                      <td className="max-w-[300px] truncate px-4 py-2">
                        <Link to={`/servers?id=${encodeURIComponent(s.server_id)}`} className="hover:text-teal">
                          {s.server_name ?? s.server_id}
                        </Link>
                      </td>
                      <td className="px-2 py-2">{s.online ? <Badge tone="lime">онлайн</Badge> : <span className="text-faint">—</span>}</td>
                      <td className="px-2 py-2 text-right num">{s.time_played_secs !== null ? fmtDuration(s.time_played_secs) : "—"}</td>
                      <td className="px-2 py-2 text-muted num">{fmtDateTime(s.first_seen, tz)}</td>
                      <td className="px-4 py-2 text-muted num">{fmtDateTime(s.last_seen, tz)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Card>
        <div className="space-y-4">
          <Card>
            <CardHeader title="История ников" />
            <ul className="max-h-60 divide-y divide-line overflow-y-auto">
              {v.names.length === 0 && <li className="px-4 py-3 text-xs text-muted">Нет данных</li>}
              {v.names.map((n) => (
                <li key={n.name} className="flex justify-between gap-2 px-4 py-1.5 text-[13px]">
                  <span className="truncate">{n.name}</span>
                  <span className="shrink-0 text-[11px] text-faint num">
                    {fmtDateTime(n.first_seen, tz, { hour: undefined, minute: undefined })} – {fmtDateTime(n.last_seen, tz, { hour: undefined, minute: undefined })}
                  </span>
                </li>
              ))}
            </ul>
          </Card>
          <SteamLink v={v} />
        </div>
      </div>
    </div>
  );
}

function SteamLink({ v }: { v: BmPlayerView }) {
  const qc = useQueryClient();
  const demo = useIsDemo();
  const [input, setInput] = useState("");
  const link = useMutation({
    mutationFn: () => api.linkBmToSteam(v.player.bm_id, input),
    onSuccess: (n) => {
      qc.invalidateQueries({ queryKey: ["bm-player", v.player.bm_id] });
      n.forEach((x) => (x.level === "warning" ? toast.warning(x.text) : toast.success(x.text)));
      setInput("");
    },
    onError: (e) => toast.error(e.message),
  });
  return (
    <Card>
      <CardHeader title="Профиль Steam" subtitle="BattleMetrics не раскрывает SteamID — свяжите вручную, если знаете" />
      <div className="space-y-2 p-3 text-[13px]">
        {v.steam ? (
          <Link to={`/players/${v.steam.steam_id}`} className="flex items-center gap-2 hover:text-teal">
            <Avatar name={v.steam.persona_name} url={v.steam.avatar_url} size={26} demo={v.steam.is_demo} />
            <span>{v.steam.persona_name ?? v.steam.steam_id}</span>
            <span className="text-[11px] text-faint num">{v.steam.steam_id}</span>
          </Link>
        ) : (
          <div className="flex gap-2">
            <Input value={input} onChange={(e) => setInput(e.target.value)} placeholder="Ссылка на профиль Steam или SteamID64" disabled={demo} aria-label="Профиль Steam" />
            <Button onClick={() => link.mutate()} loading={link.isPending} disabled={!input.trim() || demo}>
              <Link2 className="size-3.5" /> Связать
            </Button>
          </div>
        )}
      </div>
    </Card>
  );
}

function Teammates({ v }: { v: BmPlayerView }) {
  const tz = useTz();
  const [open, setOpen] = useState<string | null>(null);
  const [minConf, setMinConf] = useState<"all" | "medium">("all");
  const list = v.teammates.filter((t) => minConf === "all" || t.confidence !== "low");
  return (
    <Card>
      <CardHeader
        title="Возможные тиммейты"
        icon={<UsersRound className="size-4" />}
        subtitle={v.teammates_note}
        actions={<Segmented label="Уверенность" value={minConf} onChange={setMinConf} options={[{ value: "all", label: "Все" }, { value: "medium", label: "Средняя+" }]} />}
      />
      {v.watched_servers === 0 && !v.player.is_demo && (
        <Notice tone="warning" className="m-3">
          Нет наблюдаемых серверов. Откройте «Серверы», выберите сервер, где играет этот игрок, и включите «Наблюдать» — так программа увидит, кто заходит и выходит вместе с ним.
        </Notice>
      )}
      {list.length === 0 ? (
        <EmptyState title="Тиммейты пока не найдены" text="Нужны повторяющиеся совместные входы/выходы на одном сервере. Данные копятся, пока идёт наблюдение." />
      ) : (
        <table className="w-full text-[13px]">
          <thead className="text-left text-[11px] tracking-wide text-faint uppercase">
            <tr>
              <th className="w-6" />
              <th className="px-2 py-2 font-medium">Игрок</th>
              <th className="px-2 py-2 text-right font-medium" title="Вошли на сервер с разницей ≤ 10 минут">Вошли вместе</th>
              <th className="px-2 py-2 text-right font-medium" title="Вышли с разницей ≤ 10 минут">Вышли вместе</th>
              <th className="px-2 py-2 text-right font-medium" title="Пересечение ≥ 20 минут">Совместных сессий</th>
              <th className="px-2 py-2 text-right font-medium" title="Доля сессий цели на общих серверах, где совпал вход или выход">Совпадений</th>
              <th className="px-2 py-2 text-right font-medium">Вместе онлайн</th>
              <th className="px-2 py-2 font-medium">Последний раз</th>
              <th className="px-4 py-2 font-medium">Уверенность</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {list.map((t) => (
              <Fragment key={t.player}>
                <tr className="cursor-pointer hover:bg-raised" onClick={() => setOpen(open === t.player ? null : t.player)}>
                  <td className="pl-3 text-faint">{open === t.player ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}</td>
                  <td className="px-2 py-2">
                    <Link to={`/bm/${t.player}`} className="font-medium hover:text-teal" onClick={(e) => e.stopPropagation()}>
                      {t.name ?? t.player}
                    </Link>
                    {t.tracked && <Badge tone="teal" className="ml-1.5">отслеживается</Badge>}
                  </td>
                  <td className="px-2 py-2 text-right num">{t.joined_together}</td>
                  <td className="px-2 py-2 text-right num">{t.left_together}</td>
                  <td className="px-2 py-2 text-right num">{t.co_sessions}</td>
                  <td className="px-2 py-2 text-right num">{fmtPct(t.sync_rate)}</td>
                  <td className="px-2 py-2 text-right num">{fmtDuration(t.overlap_secs)}</td>
                  <td className="px-2 py-2 text-muted num">{fmtRelative(t.last_together)}</td>
                  <td className="px-4 py-2">
                    <Badge tone={CONF[t.confidence].tone}>{CONF[t.confidence].label}</Badge>
                  </td>
                </tr>
                {open === t.player && (
                  <tr>
                    <td />
                    <td colSpan={8} className="px-2 pb-3">
                      <div className="rounded-md border border-line bg-panel-2 p-2 text-[12px]">
                        <div className="mb-1 text-[11px] text-faint uppercase">Доказательства (последние)</div>
                        <ul className="space-y-0.5">
                          {t.evidence.map((e, i) => (
                            <li key={i} className="flex flex-wrap gap-x-2 num">
                              <span>{fmtDateTime(e.start, tz)}–{fmtTime(e.end, tz)}</span>
                              <span className="text-muted">· вместе {fmtDuration(e.overlap_secs)}</span>
                              {e.joined_together && <Badge tone="lime">вошли вместе{e.join_delta_secs !== null ? ` (Δ ${Math.round(e.join_delta_secs / 60)} мин)` : ""}</Badge>}
                              {e.left_together && <Badge tone="lime">вышли вместе</Badge>}
                              <span className="text-faint">· {e.server_id}</span>
                            </li>
                          ))}
                        </ul>
                      </div>
                    </td>
                  </tr>
                )}
              </Fragment>
            ))}
          </tbody>
        </table>
      )}
      <div className="border-t border-line px-4 py-2 text-[11px] text-faint">Это гипотеза по совпадению времени, а не подтверждение знакомства.</div>
    </Card>
  );
}

function Sessions({ sessions, total }: { sessions: BmSession[]; total: number }) {
  const tz = useTz();
  const [server, setServer] = useState("");
  const servers = useMemo(() => [...new Map(sessions.map((s) => [s.server_id, s.server_name ?? s.server_id])).entries()], [sessions]);
  const list = sessions.filter((s) => !server || s.server_id === server);
  return (
    <Card>
      <CardHeader
        title="Сессии"
        subtitle={`Вход и выход с точностью до интервала опроса. Показано ${list.length} из ${total}.`}
        actions={
          <Select value={server} onChange={(e) => setServer(e.target.value)} aria-label="Фильтр по серверу" className="h-7 text-xs">
            <option value="">Все серверы</option>
            {servers.map(([id, n]) => (
              <option key={id} value={id}>
                {n}
              </option>
            ))}
          </Select>
        }
      />
      {list.length === 0 ? (
        <EmptyState title="Сессий нет" text="Включите наблюдение за сервером, где играет игрок, или обновите из BattleMetrics." />
      ) : (
        <div className="max-h-[520px] overflow-y-auto">
          <table className="w-full text-[13px]">
            <thead className="sticky top-0 bg-panel text-left text-[11px] tracking-wide text-faint uppercase">
              <tr>
                <th className="px-4 py-2 font-medium">Зашёл</th>
                <th className="px-2 py-2 font-medium">Вышел</th>
                <th className="px-2 py-2 text-right font-medium">Сколько</th>
                <th className="px-2 py-2 font-medium">Сервер</th>
                <th className="px-4 py-2 font-medium">Источник</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {list.map((s) => {
                const dur = (new Date(s.end_lower).getTime() - new Date(s.start_upper).getTime()) / 1000;
                return (
                  <tr key={s.id} className="hover:bg-raised">
                    <td className="px-4 py-1.5 num">{s.start_known ? fmtApprox(s.start_lower, s.start_upper, tz) : <span title="Наблюдение началось, когда игрок уже был онлайн">до {fmtDateTime(s.start_upper, tz)}</span>}</td>
                    <td className="px-2 py-1.5 num">
                      {s.is_open ? <Badge tone="lime">в игре</Badge> : s.end_known && s.end_upper ? fmtApprox(s.end_lower, s.end_upper, tz) : <span title="Наблюдение прервалось">после {fmtTime(s.end_lower, tz)}</span>}
                    </td>
                    <td className="px-2 py-1.5 text-right num">{s.start_known && (s.end_known || s.is_open) ? "" : "≥ "}{fmtDuration(dur)}</td>
                    <td className="max-w-[260px] truncate px-2 py-1.5">{s.server_name ?? s.server_id}</td>
                    <td className="px-4 py-1.5">
                      <Badge tone={s.source === "bm_api" ? "teal" : "neutral"}>{s.source === "bm_api" ? "история BM" : s.source === "demo" ? "демо" : "наблюдение"}</Badge>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}

function BmActivity({ bmId }: { bmId: string }) {
  const tz = useTz();
  const [preset, setPreset] = useState<RangeSpec["preset"]>("30d");
  const q = useQuery({ queryKey: ["bm-activity", bmId, preset, tz], queryFn: () => api.bmActivity(bmId, { preset }, tz) });
  const a = q.data;
  const highlight = useMemo(() => new Set(a?.typical_offline.status === "available" ? a.typical_offline.hours.map(([w, h]) => `${w}:${h}`) : []), [a]);
  return (
    <div className="space-y-4">
      <Segmented
        label="Период"
        value={preset}
        onChange={setPreset}
        options={[
          { value: "7d", label: "7 дней" },
          { value: "30d", label: "30 дней" },
          { value: "90d", label: "90 дней" },
        ]}
      />
      {q.isLoading && <LoadingBlock rows={5} />}
      {q.error && <ErrorState error={q.error} retry={() => q.refetch()} />}
      {a && (
        <>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-3">
            <Kpi label="Покрытие" value={fmtPct(a.coverage)} sub="доля периода под наблюдением" tone={a.coverage < 0.3 ? "amber" : "teal"} hint="Наблюдаемые серверы + период истории BattleMetrics" />
            <Kpi label="Онлайн" value={fmtDuration(a.online_secs)} sub={`${fmtPct(a.known_secs ? a.online_secs / a.known_secs : null)} наблюдаемого времени`} tone="lime" />
            <Kpi label="Ячеек 7×24" value={`${a.cells.filter((c) => c.state === "known").length}/168`} />
          </div>
          <Card className={cn(a.known_secs === 0 && "opacity-70")}>
            <CardHeader title="Когда играет" subtitle="Доля онлайн от наблюдаемого времени · штриховка — нет наблюдений" />
            <div className="p-4">
              <ActivityHeatmap cells={a.cells} highlight={highlight} />
            </div>
          </Card>
          <TypicalOfflineCard t={a.typical_offline} />
        </>
      )}
    </div>
  );
}
