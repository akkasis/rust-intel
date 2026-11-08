import { useMemo } from "react";
import { Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { Activity, Radar, Server, Star, Users } from "lucide-react";
import { api } from "@/lib/api";
import { useTz } from "@/app/hooks";
import { PROVIDER_NAMES, fmtDateTime, fmtRelative } from "@/lib/format";
import { Avatar, Badge, Button, Card, CardHeader, EmptyState, ErrorState, Kpi, LoadingBlock, Notice, PageHeader } from "@/components/ui";
import { EventItem, HealthBadge, TrackedRow } from "@/components/domain";
import { EChart, baseOption, chartTheme } from "@/components/charts/EChart";

export function DashboardPage() {
  const tz = useTz();
  const q = useQuery({ queryKey: ["dashboard"], queryFn: api.dashboard, refetchInterval: 30_000 });
  const option = useMemo(() => {
    const t = chartTheme();
    const days = q.data?.events_per_day ?? [];
    return {
      ...baseOption(),
      grid: { left: 32, right: 8, top: 12, bottom: 22 },
      xAxis: { type: "category", data: days.map((d) => d.date.slice(5)), axisLine: { lineStyle: { color: t.line } }, axisTick: { show: false } },
      yAxis: { type: "value", minInterval: 1, splitLine: { lineStyle: { color: t.line } } },
      series: [{ type: "bar", name: "События игроков", data: days.map((d) => d.count), itemStyle: { color: t.teal, borderRadius: [3, 3, 0, 0] }, barMaxWidth: 18 }],
    };
  }, [q.data]);

  if (q.isLoading) return <LoadingBlock rows={8} />;
  if (q.error) return <ErrorState error={q.error} retry={() => q.refetch()} />;
  const d = q.data!;
  const online = d.tracked.filter((t) => t.last_state === "online").length;
  const unknown = d.tracked.filter((t) => !t.last_state || t.last_state === "unknown").length;
  const providers = ["steam", "battlemetrics", "battlemetrics_ru", ...(d.mode === "demo" ? ["demo"] : [])];

  return (
    <div className="space-y-4">
      <PageHeader
        title="Обзор"
        subtitle={d.tracker.last_cycle_at ? `Последний цикл трекера ${fmtRelative(d.tracker.last_cycle_at)} · интервал ${d.tracker.interval_secs} с` : "Трекер ещё не выполнял опрос в этой сессии"}
        actions={
          <>
            <Link to="/help">
              <Button>Инструкция</Button>
            </Link>
            <Link to="/search">
              <Button variant="primary">Найти игрока</Button>
            </Link>
          </>
        }
      />
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
        <Kpi label="Игроков в базе" value={d.players_total} sub={`${d.favorites.length} в избранном`} />
        <Kpi label="Отслеживается" value={d.tracked.length} sub={d.tracker.paused ? "трекер на паузе" : "опрос включён"} tone={d.tracker.paused ? "amber" : undefined} />
        <Kpi label="Онлайн сейчас" value={online} sub={`${unknown} — неизвестно`} tone="lime" hint="Только наблюдаемые источники. «Неизвестно» — источник не ответил или игрок ещё не опрашивался." />
        <Kpi label="Новые события" value={d.unread_events} sub="непрочитанные" tone="teal" />
        <Kpi label="Отряды" value={d.squads.length} sub={`${d.followed_servers.length} серверов в отслеживаемых`} />
      </div>

      <div className="grid gap-4 xl:grid-cols-[1.35fr_1fr]">
        <Card>
          <CardHeader title="Последние события" icon={<Activity className="size-4" />} actions={<Link to="/tracker" className="text-xs text-teal hover:underline">Журнал →</Link>} />
          {d.recent_events.length === 0 ? (
            <EmptyState title="Событий пока нет" text="Добавьте игроков в Live-трекер — события появятся после двух последовательных опросов." />
          ) : (
            <ul className="divide-y divide-line">{d.recent_events.map((e) => <EventItem key={e.id} e={e} tz={tz} />)}</ul>
          )}
        </Card>
        <div className="space-y-4">
          <Card>
            <CardHeader title="Отслеживаемые" icon={<Radar className="size-4" />} subtitle={d.tracker.note || undefined} />
            <div className="p-2">
              {d.tracked.length === 0 ? (
                <EmptyState title="Никто не отслеживается" text="Откройте профиль игрока и включите «Отслеживать» — опрос начнётся только после явного согласия." />
              ) : (
                d.tracked.map((t) => <TrackedRow key={t.steam_id} t={t} tz={tz} />)
              )}
            </div>
          </Card>
          <Card>
            <CardHeader title="Состояние источников" />
            <ul className="divide-y divide-line text-[13px]">
              {providers.map((p) => {
                const h = d.health.find((x) => x.provider === p);
                return (
                  <li key={p} className="flex items-center justify-between gap-2 px-4 py-2">
                    <span>{PROVIDER_NAMES[p] ?? p}</span>
                    <span className="flex items-center gap-2 text-[11px] text-faint">
                      {h?.last_success_at && <span>успех {fmtRelative(h.last_success_at)}</span>}
                      <HealthBadge h={h} />
                    </span>
                  </li>
                );
              })}
            </ul>
          </Card>
        </div>
      </div>

      <div className="grid gap-4 xl:grid-cols-3">
        <Card className="xl:col-span-1">
          <CardHeader title="Избранное" icon={<Star className="size-4" />} />
          <div className="p-2">
            {d.favorites.length === 0 ? (
              <EmptyState title="Пусто" text="Отметьте игрока звёздочкой в профиле." />
            ) : (
              d.favorites.map((p) => (
                <Link key={p.steam_id} to={`/players/${p.steam_id}`} className="flex items-center gap-2.5 rounded-md px-2 py-1.5 hover:bg-raised">
                  <Avatar name={p.persona_name} url={p.avatar_url} size={26} demo={p.is_demo} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13px] font-medium">{p.persona_name ?? p.steam_id}</div>
                    <div className="truncate text-[11px] text-faint num">{p.steam_id}</div>
                  </div>
                  {p.steam_in_game && <Badge tone="lime">в Rust</Badge>}
                </Link>
              ))
            )}
          </div>
        </Card>
        <Card>
          <CardHeader title="События игроков за 14 дней" subtitle="Входы, выходы, смены ника/сервера" />
          <div className="px-2 pb-2">
            {d.events_per_day.length ? <EChart option={option} height={190} label="Столбчатая диаграмма событий по дням" /> : <EmptyState title="Нет данных" />}
          </div>
        </Card>
        <Card>
          <CardHeader title="Отслеживаемые серверы" icon={<Server className="size-4" />} actions={<Link to="/servers" className="text-xs text-teal hover:underline">Все →</Link>} />
          <ul className="divide-y divide-line">
            {d.followed_servers.length === 0 && <EmptyState title="Нет серверов" text="Отметьте серверы в «Серверах»." />}
            {d.followed_servers.map((s) => (
              <li key={s.id} className="flex items-center justify-between gap-2 px-4 py-2">
                <div className="min-w-0">
                  <div className="truncate text-[13px]">{s.name}</div>
                  <div className="text-[11px] text-faint">
                    {s.next_wipe_at ? `вайп ${fmtDateTime(s.next_wipe_at, tz)}` : "вайп: нет данных"}
                  </div>
                </div>
                <span className="text-xs text-muted num" title={s.players === null ? "Источник не подтвердил онлайн" : undefined}>
                  {s.players ?? "—"}/{s.max_players ?? "—"}
                </span>
              </li>
            ))}
          </ul>
        </Card>
      </div>

      {d.squads.length > 0 && (
        <Card>
          <CardHeader title="Отряды" icon={<Users className="size-4" />} />
          <div className="flex flex-wrap gap-2 p-3">
            {d.squads.map((s) => (
              <Link key={s.id} to={`/squads/${s.id}`} className="flex items-center gap-2 rounded-md border border-line px-3 py-1.5 hover:border-line-strong">
                <span className="size-2.5 rounded-full" style={{ background: s.color }} />
                <span className="text-[13px]">{s.name}</span>
                <span className="text-[11px] text-faint">{s.member_count}</span>
              </Link>
            ))}
          </div>
        </Card>
      )}
      <Notice tone="info">
        Наблюдения ведутся, только пока RID запущен (в том числе в трее). Отсутствие игрока на наблюдаемых серверах не означает отсутствие в Rust.
      </Notice>
    </div>
  );
}
