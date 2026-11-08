import { useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Eye, ExternalLink, RefreshCw, Star, Users } from "lucide-react";
import { api } from "@/lib/api";
import type { Server } from "@/lib/types";
import { PROVIDER_NAMES, fmtDateTime, fmtRelative } from "@/lib/format";
import { openExternal } from "@/lib/external";
import { useTz } from "@/app/hooks";
import { Avatar, Badge, Button, Card, CardHeader, EmptyState, ErrorState, Input, LoadingBlock, Notice, PageHeader, ProvenanceChip, Segmented, Switch, cn } from "@/components/ui";
import { EChart, baseOption, chartTheme } from "@/components/charts/EChart";

const TRUST: Record<string, { label: string; tone: "teal" | "amber" | "danger" | "violet" | "neutral"; hint: string }> = {
  confirmed: { label: "подтверждён", tone: "teal", hint: "Источник подтверждает онлайн" },
  suspicious: { label: "подозрительный", tone: "amber", hint: "Онлайн не подтверждён источником — число игроков скрыто" },
  mirror: { label: "зеркало", tone: "amber", hint: "Зеркало другого сервера" },
  farm: { label: "ферма", tone: "danger", hint: "Накрутка/ферма по оценке источника" },
  demo: { label: "демо", tone: "violet", hint: "Вымышленный сервер" },
};

export function ServersPage() {
  const tz = useTz();
  const qc = useQueryClient();
  const [q, setQ] = useState("");
  const [followed, setFollowed] = useState<"all" | "followed">("all");
  const [params] = useSearchParams();
  const [sel, setSel] = useState<string | null>(params.get("id"));
  const watch = useQuery({ queryKey: ["server-watch"], queryFn: api.serverWatch });
  const watched = new Set((watch.data ?? []).filter((w) => w.enabled).map((w) => w.server_id));
  const servers = useQuery({ queryKey: ["servers"], queryFn: () => api.servers() });
  const refresh = useMutation({
    mutationFn: () => api.refreshServers(q || undefined),
    onSuccess: (r) => {
      qc.invalidateQueries({ queryKey: ["servers"] });
      r.errors.forEach((e) => toast.warning(e));
      if (r.fetched) toast.success(`Обновлено серверов: ${r.fetched}`);
    },
    onError: (e) => toast.error(e.message),
  });
  const follow = useMutation({ mutationFn: (s: Server) => api.setServerFollowed(s.id, !s.followed), onSuccess: () => qc.invalidateQueries({ queryKey: ["servers"] }) });
  const rows = useMemo(
    () => (servers.data ?? []).filter((s) => (followed === "followed" ? s.followed : true) && (!q || s.name.toLowerCase().includes(q.toLowerCase()) || s.address?.includes(q))),
    [servers.data, q, followed],
  );
  const current = rows.find((s) => s.id === sel) ?? rows[0];
  return (
    <div className="space-y-4">
      <PageHeader
        title="Серверы"
        subtitle="Серверный уровень: онлайн, вайпы, история заполненности. Это не история конкретных игроков."
        actions={
          <Button onClick={() => refresh.mutate()} loading={refresh.isPending}>
            <RefreshCw className="size-3.5" /> Обновить каталог
          </Button>
        }
      />
      <div className="flex flex-wrap gap-2">
        <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Название или адрес (для запроса к источнику — от 3 символов)" className="max-w-md" aria-label="Поиск серверов" />
        <Segmented label="Фильтр" value={followed} onChange={setFollowed} options={[{ value: "all", label: "Все" }, { value: "followed", label: "Отслеживаемые" }]} />
      </div>
      <div className="grid gap-4 xl:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)]">
        <Card>
          {servers.isLoading && <LoadingBlock />}
          {servers.error && <ErrorState error={servers.error} retry={() => servers.refetch()} />}
          {servers.data && rows.length === 0 && (
            <EmptyState title="Серверов нет" text="Подключите battlemetrics.ru (бесплатный ключ) или BattleMetrics.com в «Интеграциях» и нажмите «Обновить каталог»." />
          )}
          {rows.length > 0 && (
            <div className="max-h-[calc(100vh-240px)] overflow-y-auto">
              <table className="w-full text-[13px]">
                <thead className="sticky top-0 bg-panel text-left text-[11px] tracking-wide text-faint uppercase">
                  <tr>
                    <th className="w-8 px-3 py-2" />
                    <th className="px-2 py-2 font-medium">Сервер</th>
                    <th className="px-2 py-2 text-right font-medium">Игроки</th>
                    <th className="px-2 py-2 font-medium">Доверие</th>
                    <th className="px-3 py-2 font-medium">Вайп</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-line">
                  {rows.map((s) => {
                    const t = TRUST[s.trust ?? ""];
                    return (
                      <tr key={s.id} onClick={() => setSel(s.id)} className={cn("cursor-pointer hover:bg-raised", current?.id === s.id && "bg-raised")}>
                        <td className="px-3 py-2">
                          <button type="button" onClick={(e) => (e.stopPropagation(), follow.mutate(s))} aria-label={s.followed ? "Не отслеживать" : "Отслеживать"}>
                            <Star className={cn("size-4", s.followed ? "fill-amber text-amber" : "text-faint")} />
                          </button>
                        </td>
                        <td className="max-w-0 px-2 py-2">
                          <div className="flex items-center gap-1.5 truncate font-medium">
                            {s.name}
                            {watched.has(s.id) && <Eye className="size-3.5 shrink-0 text-teal" aria-label="наблюдается" />}
                          </div>
                          <div className="truncate text-[11px] text-faint">
                            {s.address ?? "адрес не указан"} · {s.country ?? "—"} · {PROVIDER_NAMES[s.provider] ?? s.provider}
                          </div>
                        </td>
                        <td className="px-2 py-2 text-right num" title={s.players === null ? "Источник не подтвердил онлайн" : undefined}>
                          {s.players ?? "—"}/{s.max_players ?? "—"}
                          {!!s.queue && <div className="text-[10px] text-amber">очередь {s.queue}</div>}
                        </td>
                        <td className="px-2 py-2">{t ? <Badge tone={t.tone} title={t.hint}>{t.label}</Badge> : <span className="text-faint">—</span>}</td>
                        <td className="px-3 py-2 text-[11px] text-muted num">
                          {s.last_wipe_at ? `был ${fmtDateTime(s.last_wipe_at, tz)}` : "нет данных"}
                          {s.next_wipe_at && <div className="text-teal">след. {fmtDateTime(s.next_wipe_at, tz)}</div>}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
        </Card>
        {current && (
          <div className="space-y-4">
            <ServerDetail s={current} />
            <OnlineNow s={current} />
          </div>
        )}
      </div>
    </div>
  );
}

function ServerDetail({ s }: { s: Server }) {
  const tz = useTz();
  const [period, setPeriod] = useState<"24h" | "7d" | "30d">("7d");
  const q = useQuery({ queryKey: ["population", s.id, period], queryFn: () => api.population(s.id, period) });
  const option = useMemo(() => {
    const t = chartTheme();
    const pts = q.data?.points ?? [];
    return {
      ...baseOption(),
      grid: { left: 40, right: 10, top: 16, bottom: 26 },
      xAxis: { type: "time", axisLine: { lineStyle: { color: t.line } }, splitLine: { show: false } },
      yAxis: { type: "value", max: s.max_players ?? undefined, splitLine: { lineStyle: { color: t.line } } },
      tooltip: { ...(baseOption().tooltip as object), valueFormatter: (v: number) => `${Math.round(v)} игроков` },
      series: [
        {
          type: "line",
          name: "Онлайн сервера",
          showSymbol: false,
          connectNulls: false,
          data: pts.map((p) => [p.t, p.players]),
          lineStyle: { color: t.teal, width: 1.5 },
          areaStyle: { color: { type: "linear", x: 0, y: 0, x2: 0, y2: 1, colorStops: [{ offset: 0, color: t.teal + "55" }, { offset: 1, color: t.teal + "00" }] } },
        },
      ],
    };
  }, [q.data, s.max_players]);
  return (
    <Card className="h-fit">
      <CardHeader
        title={s.name}
        subtitle={`${s.map ?? "карта неизвестна"} · обновлено ${fmtRelative(s.fetched_at)}`}
        actions={
          s.source_url && (
            <Button size="iconSm" variant="ghost" onClick={() => openExternal(s.source_url!)} aria-label="Открыть источник">
              <ExternalLink className="size-3.5" />
            </Button>
          )
        }
      />
      <div className="space-y-3 p-4">
        <div className="flex items-center justify-between">
          <Segmented label="Период" value={period} onChange={setPeriod} options={[{ value: "24h", label: "24 ч" }, { value: "7d", label: "7 дней" }, { value: "30d", label: "30 дней" }]} />
          {q.data && <ProvenanceChip p={q.data.provenance} />}
        </div>
        {q.isLoading && <LoadingBlock rows={3} />}
        {q.error && <ErrorState error={q.error} retry={() => q.refetch()} />}
        {q.data && (q.data.points.length ? <EChart option={option} height={240} label="История онлайна сервера" /> : <EmptyState title="Нет истории" text="Источник не вернул точек за период. Шаги без успешных опросов пропускаются, а не считаются нулём." />)}
        <dl className="grid grid-cols-2 gap-2 text-[13px]">
          <div>
            <dt className="text-[11px] text-faint">Адрес</dt>
            <dd className="num">{s.address ?? "—"}</dd>
          </div>
          <div>
            <dt className="text-[11px] text-faint">Последний вайп</dt>
            <dd className="num">{fmtDateTime(s.last_wipe_at, tz)}</dd>
          </div>
          <div>
            <dt className="text-[11px] text-faint">Следующий вайп</dt>
            <dd className="num">{s.next_wipe_at ? fmtDateTime(s.next_wipe_at, tz) : "не задокументирован"}</dd>
          </div>
          <div>
            <dt className="text-[11px] text-faint">Источник</dt>
            <dd>{PROVIDER_NAMES[s.provider] ?? s.provider}</dd>
          </div>
        </dl>
        <Notice tone="info">История онлайна сервера — агрегат по серверу. Она не показывает, какие игроки были на сервере.</Notice>
      </div>
    </Card>
  );
}

function OnlineNow({ s }: { s: Server }) {
  const tz = useTz();
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["server-online", s.id], queryFn: () => api.serverOnline(s.id), retry: false, refetchInterval: 60_000 });
  const watch = useMutation({
    mutationFn: (v: boolean) => api.setServerWatch(s.id, v),
    onSuccess: (_, v) => {
      qc.invalidateQueries({ queryKey: ["server-watch"] });
      qc.invalidateQueries({ queryKey: ["server-online", s.id] });
      toast.success(v ? "Наблюдение включено: онлайн сервера будет записываться каждые пару минут" : "Наблюдение выключено");
    },
    onError: (e) => toast.error(e.message),
  });
  const [f, setF] = useState("");
  const list = (q.data?.players ?? []).filter((p) => !f || p.name.toLowerCase().includes(f.toLowerCase()));
  return (
    <Card>
      <CardHeader
        title="Онлайн сейчас"
        icon={<Users className="size-4" />}
        subtitle={q.data ? `${q.data.players.length} игроков · ${fmtRelative(q.data.fetched_at)}` : "Кто на сервере прямо сейчас"}
        actions={
          <label className="flex items-center gap-2 text-xs text-muted" title="Записывать онлайн сервера, чтобы видеть входы/выходы всех игроков и находить тиммейтов">
            Наблюдать
            <Switch checked={!!q.data?.watched} onChange={(v) => watch.mutate(v)} label="Наблюдать за сервером" disabled={watch.isPending} />
          </label>
        }
      />
      <div className="p-3">
        {q.isLoading && <LoadingBlock rows={3} />}
        {q.error && <Notice tone="warning">{q.error.message}</Notice>}
        {q.data?.note && <Notice tone="info" className="mb-2">{q.data.note}</Notice>}
        {q.data && q.data.players.length > 0 && (
          <>
            <Input value={f} onChange={(e) => setF(e.target.value)} placeholder="Фильтр по нику" className="mb-2 h-7 text-xs" aria-label="Фильтр игроков" />
            <ul className="max-h-[420px] divide-y divide-line overflow-y-auto">
              {list.map((p) => (
                <li key={p.bm_id}>
                  <Link to={`/bm/${p.bm_id}`} className="flex items-center gap-2.5 px-1 py-1.5 hover:bg-raised">
                    <Avatar name={p.name} size={22} />
                    <span className="min-w-0 flex-1 truncate text-[13px]">{p.name}</span>
                    {p.tracked && <Badge tone="teal">отслеживается</Badge>}
                    <span className="text-[11px] text-faint num" title={p.since_known ? "Время входа (по опросам)" : "Был онлайн уже при начале наблюдения"}>
                      {p.since ? `${p.since_known ? "с" : "до"} ${fmtDateTime(p.since, tz)}` : ""}
                    </span>
                  </Link>
                </li>
              ))}
            </ul>
          </>
        )}
        {q.data && q.data.players.length === 0 && <EmptyState title="Никого онлайн" />}
      </div>
    </Card>
  );
}
