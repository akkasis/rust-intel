import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, MoonStar } from "lucide-react";
import { api } from "@/lib/api";
import type { ActivityReport, RangeSpec, Session, TypicalOffline } from "@/lib/types";
import { PROVIDER_NAMES, WEEKDAYS, fmtApprox, fmtDuration, fmtPct, fmtTime } from "@/lib/format";
import { useTz } from "@/app/hooks";
import { Badge, Card, CardHeader, EmptyState, ErrorState, Hint, Kpi, LoadingBlock, Notice, Segmented, Select, Switch } from "@/components/ui";
import { ActivityHeatmap } from "@/components/charts/Heatmap";
import { EChart, baseOption, chartTheme } from "@/components/charts/EChart";

type Preset = RangeSpec["preset"];

export function ActivityView({ steamId }: { steamId: string }) {
  const tz = useTz();
  const [preset, setPreset] = useState<Preset>("30d");
  const [server, setServer] = useState<string>("");
  const [showOffline, setShowOffline] = useState(true);
  const q = useQuery({
    queryKey: ["activity", steamId, preset, tz, server],
    queryFn: () => api.activity(steamId, { preset }, tz, server || null),
  });
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <Segmented
          label="Период"
          value={preset}
          onChange={setPreset}
          options={[
            { value: "24h", label: "24 ч" },
            { value: "7d", label: "7 дней" },
            { value: "30d", label: "30 дней" },
            { value: "90d", label: "90 дней" },
          ]}
        />
        <Select value={server} onChange={(e) => setServer(e.target.value)} aria-label="Фильтр по серверу">
          <option value="">Все наблюдаемые серверы</option>
          {(q.data?.servers ?? []).map(([id, name]) => (
            <option key={id} value={id}>
              {name ?? id}
            </option>
          ))}
        </Select>
        <span className="text-xs text-muted">
          Часовой пояс: <span className="num text-fg">{tz}</span> <Hint text="Учитываются правила IANA, включая переходы на летнее время. Моменты хранятся в UTC." />
        </span>
      </div>
      {q.isLoading && <LoadingBlock rows={6} />}
      {q.error && <ErrorState error={q.error} retry={() => q.refetch()} />}
      {q.data && <ActivityBody r={q.data} tz={tz} showOffline={showOffline} setShowOffline={setShowOffline} />}
    </div>
  );
}

function offlineSet(t: TypicalOffline): Set<string> {
  return t.status === "available" ? new Set(t.hours.map(([w, h]) => `${w}:${h}`)) : new Set();
}

function ActivityBody({ r, tz, showOffline, setShowOffline }: { r: ActivityReport; tz: string; showOffline: boolean; setShowOffline: (v: boolean) => void }) {
  const a = r.aggregate;
  const knownCells = a.cells.filter((c) => c.state === "known").length;
  const unknownCells = a.cells.filter((c) => c.state === "unknown").length;
  const highlight = useMemo(() => (showOffline ? offlineSet(a.typical_offline) : new Set<string>()), [a.typical_offline, showOffline]);

  const daily = useMemo(() => {
    const t = chartTheme();
    const days = a.daily;
    return {
      ...baseOption(),
      legend: { top: 0, right: 0, textStyle: { color: t.muted }, itemWidth: 10, itemHeight: 8 },
      grid: { left: 36, right: 10, top: 28, bottom: 24 },
      xAxis: { type: "category", data: days.map((d) => d.date.slice(5)), axisLine: { lineStyle: { color: t.line } }, axisTick: { show: false } },
      yAxis: { type: "value", name: "ч", nameTextStyle: { color: t.faint }, splitLine: { lineStyle: { color: t.line } }, max: 24 },
      tooltip: {
        ...(baseOption().tooltip as object),
        valueFormatter: (v: number) => `${v.toFixed(1)} ч`,
      },
      series: [
        { name: "Онлайн (наблюдалось)", type: "bar", stack: "d", data: days.map((d) => d.online_secs / 3600), itemStyle: { color: t.teal }, barMaxWidth: 16 },
        { name: "Наблюдалось офлайн", type: "bar", stack: "d", data: days.map((d) => (d.known_secs - d.online_secs) / 3600), itemStyle: { color: t.line }, barMaxWidth: 16 },
        { name: "Нет наблюдений", type: "bar", stack: "d", data: days.map((d) => Math.max(0, 24 - d.known_secs / 3600)), itemStyle: { color: "transparent", borderColor: t.unknown, borderType: "dashed", borderWidth: 1 }, barMaxWidth: 16 },
      ],
    };
  }, [a.daily]);

  const weekday = useMemo(() => {
    const t = chartTheme();
    return {
      ...baseOption(),
      grid: { left: 36, right: 10, top: 12, bottom: 24 },
      xAxis: { type: "category", data: WEEKDAYS, axisLine: { lineStyle: { color: t.line } }, axisTick: { show: false } },
      yAxis: { type: "value", max: 1, axisLabel: { formatter: (v: number) => `${Math.round(v * 100)}%` }, splitLine: { lineStyle: { color: t.line } } },
      tooltip: {
        ...(baseOption().tooltip as object),
        formatter: (ps: { dataIndex: number }[]) => {
          const w = a.weekdays[ps[0]?.dataIndex ?? 0];
          if (!w) return "";
          return w.ratio === null ? `${WEEKDAYS[w.weekday]}: недостаточно наблюдений` : `${WEEKDAYS[w.weekday]}: ${fmtPct(w.ratio)}<br/>онлайн ${fmtDuration(w.online_secs)} из ${fmtDuration(w.known_secs)}`;
        },
      },
      series: [
        {
          type: "bar",
          data: a.weekdays.map((w) => (w.ratio === null ? { value: 0, itemStyle: { color: t.unknown } } : w.ratio)),
          itemStyle: { color: t.lime, borderRadius: [3, 3, 0, 0] },
          barMaxWidth: 28,
        },
      ],
    };
  }, [a.weekdays]);

  return (
    <>
      {r.is_demo && <Notice tone="demo">Демо-данные: сессии и покрытие сгенерированы и не относятся к реальным людям.</Notice>}
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
        <Kpi label="Покрытие" value={fmtPct(a.coverage)} sub={`${fmtDuration(a.known_secs)} из ${fmtDuration(a.range_secs)}`} tone={a.coverage < 0.3 ? "amber" : "teal"} hint="Доля периода, когда источник действительно наблюдал игрока. Остальное — неизвестно." />
        <Kpi label="Онлайн наблюдался" value={fmtDuration(a.online_secs)} sub={`${fmtPct(a.known_secs ? a.online_secs / a.known_secs : null)} наблюдаемого времени`} tone="lime" />
        <Kpi label="Сессий" value={r.sessions.length + (r.sessions_truncated ? "+" : "")} sub={r.sources.map((s) => PROVIDER_NAMES[s] ?? s).join(", ") || "нет источников"} />
        <Kpi label="Ячеек 7×24" value={`${knownCells}/168`} sub={`${unknownCells} без наблюдений`} hint={`Ячейка считается известной при ≥ ${fmtDuration(a.min_known_secs_per_cell)} наблюдений за период`} />
        <Kpi label="Шаг опроса" value={r.poll_interval_secs ? fmtDuration(r.poll_interval_secs) : "—"} sub="точность границ сессий" hint="Вход/выход определяется между двумя опросами; точное время неизвестно." />
      </div>

      {a.known_secs === 0 ? (
        <Card>
          <EmptyState
            icon={<AlertTriangle className="size-7 text-amber" />}
            title="Нет наблюдений за этот период"
            text="Включите отслеживание игрока (Live-трекер) или подключите источник с историей сессий. Пустой график здесь означает «неизвестно», а не «не играл»."
          />
        </Card>
      ) : (
        <>
          <Card>
            <CardHeader
              title="Тепловая карта: дни недели × часы"
              subtitle="Цвет — доля онлайн от НАБЛЮДАЕМОГО времени ячейки; штриховка — нет наблюдений"
              actions={
                <label className="flex items-center gap-2 text-xs text-muted">
                  <Switch checked={showOffline} onChange={setShowOffline} label="Подсветить типичные офлайн-часы" />
                  офлайн-часы
                </label>
              }
            />
            <div className="p-4">
              <ActivityHeatmap cells={a.cells} highlight={highlight} />
            </div>
          </Card>
          <div className="grid gap-4 xl:grid-cols-[1.4fr_1fr]">
            <Card>
              <CardHeader title="По дням" subtitle="Онлайн / наблюдалось офлайн / нет наблюдений (часы в сутках)" />
              <div className="px-2 pb-2">
                <EChart option={daily} height={230} label="Ежедневная активность с разделением на онлайн, офлайн и неизвестно" />
              </div>
            </Card>
            <Card>
              <CardHeader title="По дням недели" subtitle="Доля онлайн от наблюдаемого времени" />
              <div className="px-2 pb-2">
                <EChart option={weekday} height={230} label="Доля онлайн по дням недели" />
              </div>
            </Card>
          </div>
          <TypicalOfflineCard t={a.typical_offline} />
        </>
      )}
      <SessionsTable sessions={r.sessions} tz={tz} truncated={r.sessions_truncated} />
    </>
  );
}

function groupRanges(hours: [number, number][]): Map<number, string[]> {
  const m = new Map<number, string[]>();
  for (let w = 0; w < 7; w++) {
    const hs = hours.filter(([d]) => d === w).map(([, h]) => h).sort((x, y) => x - y);
    const out: string[] = [];
    let start: number | null = null;
    let prev = -2;
    for (const h of [...hs, 99]) {
      if (h !== prev + 1) {
        if (start !== null) out.push(`${String(start).padStart(2, "0")}–${String(prev + 1).padStart(2, "0")}`);
        start = h;
      }
      prev = h;
    }
    if (out.length) m.set(w, out);
  }
  return m;
}

export function TypicalOfflineCard({ t, squad }: { t: TypicalOffline; squad?: boolean }) {
  return (
    <Card>
      <CardHeader
        icon={<MoonStar className="size-4" />}
        title={squad ? "Типичные часы, когда никто из отряда не наблюдался онлайн" : "Типичные игровые офлайн-часы"}
        subtitle="Не утверждение о сне или распорядке человека: только отсутствие на наблюдаемых источниках"
      />
      <div className="p-4 text-[13px]">
        {t.status === "insufficient_data" ? (
          <Notice tone="warning">
            Недостаточно данных: известно {t.known_cells} из {t.required_cells} требуемых ячеек (нужно ≥ 3 разных дней наблюдения в каждой). Прогноз не строится, чтобы не выдавать догадку за факт.
          </Notice>
        ) : t.hours.length === 0 ? (
          <div className="text-muted">Нет часов, в которые {squad ? "отряд" : "игрок"} стабильно не наблюдался онлайн.</div>
        ) : (
          <div className="grid gap-1.5 sm:grid-cols-2 xl:grid-cols-4">
            {[...groupRanges(t.hours)].map(([w, ranges]) => (
              <div key={w} className="flex gap-2">
                <span className="w-6 text-muted">{WEEKDAYS[w]}</span>
                <span className="num">{ranges.join(", ")}</span>
              </div>
            ))}
          </div>
        )}
        <p className="mt-3 text-[11px] text-faint">
          Порог: доля {squad ? "«все наблюдались офлайн»" : "онлайн"} {squad ? "≥" : "≤"} {t.status === "available" ? fmtPct(t.threshold) : "—"} при ≥ 3 днях наблюдений. Отсутствие на наблюдаемом сервере ≠ отсутствие в Rust.
        </p>
      </div>
    </Card>
  );
}

function SessionsTable({ sessions, tz, truncated }: { sessions: Session[]; tz: string; truncated: boolean }) {
  return (
    <Card>
      <CardHeader title="Сессии" subtitle="Границы из опросов показаны интервалом неопределённости" actions={truncated ? <Badge tone="amber">показаны последние 500</Badge> : undefined} />
      {sessions.length === 0 ? (
        <EmptyState title="Сессий за период нет" text="Это означает «не наблюдался», а не «не играл»." />
      ) : (
        <div className="max-h-[360px] overflow-y-auto">
          <table className="w-full text-[13px]">
            <thead className="sticky top-0 bg-panel text-left text-[11px] tracking-wide text-faint uppercase">
              <tr>
                <th className="px-4 py-2 font-medium">Начало</th>
                <th className="px-2 py-2 font-medium">Конец</th>
                <th className="px-2 py-2 font-medium">Длительность</th>
                <th className="px-2 py-2 font-medium">Сервер</th>
                <th className="px-4 py-2 font-medium">Тип</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {sessions.map((s) => {
                const dur = (new Date(s.end_lower).getTime() - new Date(s.start_upper).getTime()) / 1000;
                return (
                  <tr key={s.id} className="hover:bg-raised">
                    <td className="px-4 py-1.5 num">{fmtApprox(s.start_lower, s.start_upper, tz)}</td>
                    <td className="px-2 py-1.5 num">
                      {s.is_open ? <Badge tone="lime">идёт · с {fmtTime(s.end_lower, tz)}</Badge> : s.end_upper ? fmtApprox(s.end_lower, s.end_upper, tz) : <span title="Наблюдение оборвалось до конца сессии">после {fmtTime(s.end_lower, tz)} · ?</span>}
                    </td>
                    <td className="px-2 py-1.5 num">≥ {fmtDuration(dur)}</td>
                    <td className="max-w-[260px] truncate px-2 py-1.5">{s.server_name ?? s.server_id ?? <span className="text-faint">Steam: в Rust</span>}</td>
                    <td className="px-4 py-1.5">
                      <Badge tone={s.kind === "provider_reported" ? "teal" : "neutral"}>{s.kind === "provider_reported" ? "от провайдера" : "из опросов"}</Badge>
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
