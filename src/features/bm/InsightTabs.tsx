import { useMemo } from "react";
import { Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { CalendarClock, Fingerprint, Sparkles } from "lucide-react";
import { api } from "@/lib/api";
import { fmtDateTime, fmtDuration, fmtPct, fmtRelative } from "@/lib/format";
import { useTz } from "@/app/hooks";
import { Badge, Card, CardHeader, EmptyState, ErrorState, Kpi, LoadingBlock, Notice } from "@/components/ui";
import { EChart, baseOption, chartTheme } from "@/components/charts/EChart";

function Summary({ items }: { items: string[] }) {
  return (
    <Card className="p-4">
      <ul className="space-y-1.5 text-[13px]">
        {items.map((s, i) => (
          <li key={i} className="flex gap-2">
            <Sparkles className="mt-0.5 size-3.5 shrink-0 text-teal" />
            {s}
          </li>
        ))}
      </ul>
    </Card>
  );
}

export function RoutineTab({ bmId }: { bmId: string }) {
  const tz = useTz();
  const q = useQuery({ queryKey: ["bm-routine", bmId, tz], queryFn: () => api.bmRoutine(bmId, tz) });
  const r = q.data;
  const startsOpt = useMemo(() => {
    const t = chartTheme();
    return {
      ...baseOption(),
      grid: { left: 30, right: 8, top: 12, bottom: 24 },
      xAxis: { type: "category", data: Array.from({ length: 24 }, (_, h) => String(h).padStart(2, "0")), axisLine: { lineStyle: { color: t.line } }, axisTick: { show: false } },
      yAxis: { type: "value", minInterval: 1, splitLine: { lineStyle: { color: t.line } } },
      series: [{ type: "bar", name: "Входов", data: r?.start_hours ?? [], itemStyle: { color: t.teal, borderRadius: [3, 3, 0, 0] }, barMaxWidth: 16 }],
    };
  }, [r]);
  const fcOpt = useMemo(() => {
    const t = chartTheme();
    const f = r?.forecast ?? [];
    return {
      ...baseOption(),
      grid: { left: 36, right: 8, top: 12, bottom: 24 },
      xAxis: { type: "category", data: f.map((x) => fmtDateTime(x.at, tz, { day: undefined, month: undefined, minute: undefined }) + ":00"), axisLine: { lineStyle: { color: t.line } }, axisTick: { show: false } },
      yAxis: { type: "value", max: 1, axisLabel: { formatter: (v: number) => `${Math.round(v * 100)}%` }, splitLine: { lineStyle: { color: t.line } } },
      tooltip: { ...(baseOption().tooltip as object), formatter: (ps: { dataIndex: number }[]) => {
        const x = f[ps[0]?.dataIndex ?? 0];
        return x ? (x.p === null ? "мало данных" : `${fmtPct(x.p)} · дней наблюдения: ${x.sample_days}`) : "";
      } },
      series: [
        {
          type: "bar",
          data: f.map((x) => (x.p === null ? { value: 1, itemStyle: { color: "transparent", borderColor: t.unknown, borderType: "dashed" } } : { value: x.p, itemStyle: { color: x.p >= 0.5 ? t.lime : t.teal } })),
          barMaxWidth: 18,
        },
      ],
    };
  }, [r, tz]);
  if (q.isLoading) return <LoadingBlock rows={5} />;
  if (q.error) return <ErrorState error={q.error} retry={() => q.refetch()} />;
  if (!r) return null;
  return (
    <div className="space-y-4">
      <Summary items={r.summary} />
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
        <Kpi label="Обычно заходит" value={r.usual_start ? `${String(r.usual_start[0]).padStart(2, "0")}–${String(r.usual_start[1]).padStart(2, "0")}` : "—"} sub={tz} tone="teal" />
        <Kpi label="Типичная сессия" value={r.median_session_secs ? fmtDuration(r.median_session_secs) : "—"} sub="медиана" />
        <Kpi label="В день (когда играет)" value={r.avg_daily_secs_active_days ? fmtDuration(r.avg_daily_secs_active_days) : "—"} sub={`активных дней: ${r.active_days}`} />
        <Kpi label="Будни / выходные" value={`${r.weekday_avg_secs ? fmtDuration(r.weekday_avg_secs) : "—"} / ${r.weekend_avg_secs ? fmtDuration(r.weekend_avg_secs) : "—"}`} />
        <Kpi label="Скорее всего онлайн" value={r.next_likely_online ? fmtDateTime(r.next_likely_online, tz) : "—"} tone="lime" hint="Ближайший час с вероятностью ≥ 50% по прошлым неделям" />
      </div>
      <div className="grid gap-4 xl:grid-cols-2">
        <Card>
          <CardHeader title="Прогноз на 24 часа" subtitle="Вероятность онлайна по тем же часам прошлых недель · пунктир — мало данных" icon={<CalendarClock className="size-4" />} />
          <div className="px-2 pb-2">
            <EChart option={fcOpt} height={220} label="Прогноз вероятности онлайна по часам" />
          </div>
        </Card>
        <Card>
          <CardHeader title="Во сколько заходит" subtitle={`Только наблюдённые входы: ${r.sessions_with_known_start} из ${r.sessions}`} />
          <div className="px-2 pb-2">
            <EChart option={startsOpt} height={220} label="Распределение входов по часам" />
          </div>
        </Card>
      </div>
      <Notice tone="info">Прогноз — статистика по прошлым наблюдениям, а не гарантия. Отсутствие на наблюдаемых серверах ≠ отсутствие в Rust.</Notice>
    </div>
  );
}

export function PostWipeTab({ bmId }: { bmId: string }) {
  const tz = useTz();
  const q = useQuery({ queryKey: ["bm-postwipe", bmId, tz], queryFn: () => api.bmPostWipe(bmId, tz) });
  const p = q.data;
  const opt = useMemo(() => {
    const t = chartTheme();
    return {
      ...baseOption(),
      grid: { left: 40, right: 8, top: 12, bottom: 24 },
      xAxis: { type: "category", data: ["День вайпа", "2-й", "3-й", "4-й", "5-й", "6-й", "7-й"], axisLine: { lineStyle: { color: t.line } }, axisTick: { show: false } },
      yAxis: { type: "value", axisLabel: { formatter: (v: number) => `${(v / 3600).toFixed(0)}ч` }, splitLine: { lineStyle: { color: t.line } } },
      tooltip: { ...(baseOption().tooltip as object), valueFormatter: (v: number) => fmtDuration(v) },
      series: [{ type: "bar", data: p?.avg_daily_secs ?? [], itemStyle: { color: t.lime, borderRadius: [3, 3, 0, 0] }, barMaxWidth: 34 }],
    };
  }, [p]);
  if (q.isLoading) return <LoadingBlock rows={5} />;
  if (q.error) return <ErrorState error={q.error} retry={() => q.refetch()} />;
  if (!p) return null;
  return (
    <div className="space-y-4">
      <Summary items={p.summary} />
      {p.cycles.length === 0 ? (
        <Card>
          <EmptyState title="Нет данных о вайпах" text="Вайпы запоминаются при обновлении серверов (каталог, наблюдение, battlemetrics.ru). Через неделю-две данных станет достаточно." />
        </Card>
      ) : (
        <>
          <div className="grid gap-4 xl:grid-cols-[1.3fr_1fr]">
            <Card>
              <CardHeader title="Сколько играет по дням после вайпа" subtitle="Среднее по вайпам, в которых играл" />
              <div className="px-2 pb-2">
                <EChart option={opt} height={230} label="Средняя игра по дням после вайпа" />
              </div>
            </Card>
            <Card>
              <CardHeader title="Основной сервер по неделям" subtitle="Где провёл больше всего времени" />
              <ul className="max-h-[260px] divide-y divide-line overflow-y-auto">
                {p.main_server_by_week
                  .slice()
                  .reverse()
                  .map(([w, s, secs], i, arr) => (
                    <li key={w} className="flex justify-between gap-2 px-4 py-1.5 text-[13px]">
                      <span className="num text-muted">с {w}</span>
                      <span className="truncate">
                        {s}
                        {arr[i + 1] && arr[i + 1]![1] !== s && <Badge tone="amber" className="ml-1.5">смена</Badge>}
                      </span>
                      <span className="text-muted num">{fmtDuration(secs)}</span>
                    </li>
                  ))}
              </ul>
            </Card>
          </div>
          <Card>
            <CardHeader title="Вайпы" />
            <table className="w-full text-[13px]">
              <thead className="text-left text-[11px] tracking-wide text-faint uppercase">
                <tr>
                  <th className="px-4 py-2 font-medium">Вайп</th>
                  <th className="px-2 py-2 font-medium">Сервер</th>
                  <th className="px-2 py-2 font-medium">Первый вход</th>
                  <th className="px-4 py-2 text-right font-medium">За неделю</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {p.cycles.map((c) => (
                  <tr key={c.server_id + c.wipe_at}>
                    <td className="px-4 py-1.5 num">{fmtDateTime(c.wipe_at, tz)}</td>
                    <td className="max-w-[240px] truncate px-2 py-1.5">{c.server_id}</td>
                    <td className="px-2 py-1.5 num">{c.first_login_delay_secs === null ? <span className="text-faint">не заходил</span> : `через ${fmtDuration(c.first_login_delay_secs)}`}</td>
                    <td className="px-4 py-1.5 text-right num">{fmtDuration(c.daily_secs.reduce((a, b) => a + b, 0))}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>
        </>
      )}
    </div>
  );
}

export function AltsTab({ bmId }: { bmId: string }) {
  const tz = useTz();
  const q = useQuery({ queryKey: ["bm-alts", bmId], queryFn: () => api.bmAlts(bmId) });
  if (q.isLoading) return <LoadingBlock rows={4} />;
  if (q.error) return <ErrorState error={q.error} retry={() => q.refetch()} />;
  const list = q.data ?? [];
  return (
    <Card>
      <CardHeader
        title="Возможный новый аккаунт"
        icon={<Fingerprint className="size-4" />}
        subtitle="Игроки, впервые замеченные за 30 дней, которые заходят вместе с этим игроком и его тиммейтами и/или играют по похожему расписанию"
      />
      {list.length === 0 ? (
        <EmptyState title="Подозрительных новых аккаунтов нет" text="Нужны наблюдения серверов, где играет группа. Проверка повторяется при каждом открытии." />
      ) : (
        <ul className="divide-y divide-line">
          {list.map((a) => (
            <li key={a.bm_id} className="px-4 py-2.5">
              <div className="flex flex-wrap items-center gap-2">
                <Link to={`/bm/${a.bm_id}`} className="font-medium hover:text-teal">
                  {a.name ?? a.bm_id}
                </Link>
                <span className="text-[11px] text-faint">впервые {fmtRelative(a.first_seen)} ({fmtDateTime(a.first_seen, tz)})</span>
                <Badge tone={a.score >= 10 ? "amber" : "neutral"}>оценка {a.score.toFixed(1)}</Badge>
              </div>
              <ul className="mt-1 ml-4 list-disc text-xs text-muted">
                {a.reasons.map((r, i) => (
                  <li key={i}>{r}</li>
                ))}
                {a.with_whom.length > 0 && <li>совпадения с: {a.with_whom.join(", ")}</li>}
              </ul>
            </li>
          ))}
        </ul>
      )}
      <div className="border-t border-line px-4 py-2 text-[11px] text-faint">Это гипотеза для проверки, а не доказательство: новые игроки часто просто присоединяются к знакомым.</div>
    </Card>
  );
}
