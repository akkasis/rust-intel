import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ExternalLink, RefreshCw, Star } from "lucide-react";
import { api } from "@/lib/api";
import type { Server } from "@/lib/types";
import { PROVIDER_NAMES, fmtDateTime, fmtRelative } from "@/lib/format";
import { openExternal } from "@/lib/external";
import { useTz } from "@/app/hooks";
import { Badge, Button, Card, CardHeader, EmptyState, ErrorState, Input, LoadingBlock, Notice, PageHeader, ProvenanceChip, Segmented, cn } from "@/components/ui";
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
  const [sel, setSel] = useState<string | null>(null);
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
                          <div className="truncate font-medium">{s.name}</div>
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
        {current && <ServerDetail s={current} />}
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
