import { useMemo, useRef, useState } from "react";
import { Link } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { toast } from "sonner";
import { Bell, BellOff, CheckCheck, Pause, Play, RefreshCw, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { EVENT_TITLES, PROVIDER_NAMES, fmtDateTime, fmtRelative } from "@/lib/format";
import { useAppInfo, useTz } from "@/app/hooks";
import { Avatar, Badge, Button, Card, CardHeader, EmptyState, ErrorState, Kpi, LoadingBlock, Notice, PageHeader, Select, StateDot, Switch } from "@/components/ui";
import { EventItem, HealthBadge } from "@/components/domain";

export function TrackerPage() {
  const tz = useTz();
  const qc = useQueryClient();
  const info = useAppInfo();
  const tracked = useQuery({ queryKey: ["tracked"], queryFn: api.tracked, refetchInterval: 20_000 });
  const status = useQuery({ queryKey: ["tracker-status"], queryFn: api.trackerStatus, refetchInterval: 10_000 });
  const dash = useQuery({ queryKey: ["dashboard"], queryFn: api.dashboard });
  const [kind, setKind] = useState("");
  const [player, setPlayer] = useState("");
  const [unread, setUnread] = useState(false);
  const events = useQuery({
    queryKey: ["events", kind, player, unread],
    queryFn: () => api.events({ kind: kind || undefined, steamId: player || undefined, unreadOnly: unread, limit: 1000 }),
    refetchInterval: 20_000,
  });
  const inv = () => {
    qc.invalidateQueries({ queryKey: ["tracked"] });
    qc.invalidateQueries({ queryKey: ["dashboard"] });
    qc.invalidateQueries({ queryKey: ["events"] });
    qc.invalidateQueries({ queryKey: ["tracker-status"] });
    qc.invalidateQueries({ queryKey: ["app-info"] });
  };
  const paused = info.data?.settings.tracking_paused ?? false;
  const pause = useMutation({ mutationFn: (p: boolean) => api.setTrackingPaused(p), onSuccess: inv });
  const poll = useMutation({ mutationFn: api.pollNow, onSuccess: (n) => (inv(), toast.success(n ? `Новых событий: ${n}` : "Опрос выполнен, новых событий нет")), onError: (e) => toast.error(e.message) });
  const toggle = useMutation({ mutationFn: ({ sid, v }: { sid: string; v: boolean }) => api.setTracking(sid, v), onSuccess: inv });
  const notify = useMutation({ mutationFn: ({ sid, v }: { sid: string; v: boolean }) => api.setTrackingNotify(sid, v), onSuccess: inv });
  const remove = useMutation({ mutationFn: (sid: string) => api.removeTracking(sid), onSuccess: inv });
  const read = useMutation({ mutationFn: () => api.markEventsRead(), onSuccess: inv });
  const list = tracked.data ?? [];
  const online = list.filter((t) => t.last_state === "online").length;
  const unknown = list.filter((t) => !t.last_state || t.last_state === "unknown").length;
  const parent = useRef<HTMLDivElement>(null);
  const evs = events.data ?? [];
  const v = useVirtualizer({ count: evs.length, getScrollElement: () => parent.current, estimateSize: () => 52, overscan: 8 });
  const kinds = useMemo(() => Object.entries(EVENT_TITLES), []);

  return (
    <div className="space-y-4">
      <PageHeader
        title="Live-трекер"
        subtitle={status.data?.note || "Опрос ведётся, только пока приложение запущено (в т.ч. в трее)."}
        actions={
          <>
            <Button onClick={() => poll.mutate()} loading={poll.isPending} disabled={paused}>
              <RefreshCw className="size-3.5" /> Опросить сейчас
            </Button>
            <Button variant={paused ? "primary" : "secondary"} onClick={() => pause.mutate(!paused)}>
              {paused ? <Play className="size-3.5" /> : <Pause className="size-3.5" />}
              {paused ? "Возобновить всё" : "Пауза всего"}
            </Button>
          </>
        }
      />
      {paused && <Notice tone="warning">Трекер на паузе: опросы не выполняются, данные за это время будут «неизвестно».</Notice>}
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
        <Kpi label="Отслеживается" value={list.length} />
        <Kpi label="Онлайн" value={online} tone="lime" hint="Наблюдается источником прямо сейчас" />
        <Kpi label="Неизвестно" value={unknown} tone={unknown ? "amber" : undefined} hint="Источник не ответил или опроса ещё не было. Это НЕ «офлайн»." />
        <Kpi label="Последний цикл" value={status.data?.last_cycle_at ? fmtRelative(status.data.last_cycle_at) : "—"} sub={status.data?.next_cycle_at ? `следующий ${fmtDateTime(status.data.next_cycle_at, tz)}` : undefined} />
        <Kpi label="Источники" value={status.data?.sources.length ?? 0} sub={(status.data?.sources ?? []).map((s) => PROVIDER_NAMES[s] ?? s).join(", ") || "не настроены"} />
      </div>
      <div className="grid gap-4 xl:grid-cols-[1fr_1.25fr]">
        <Card>
          <CardHeader title="Отслеживаемые игроки" subtitle="Добавление — из профиля игрока (явное включение)" />
          {tracked.isLoading && <LoadingBlock />}
          {tracked.error && <ErrorState error={tracked.error} retry={() => tracked.refetch()} />}
          {tracked.data && list.length === 0 && <EmptyState title="Список пуст" text="Откройте профиль игрока и включите «Live-трекер»." action={<Link to="/search"><Button size="sm">Найти игрока</Button></Link>} />}
          <ul className="divide-y divide-line">
            {list.map((t) => (
              <li key={t.steam_id} className="flex items-center gap-3 px-4 py-2">
                <StateDot state={t.last_state} />
                <Avatar name={t.persona_name} url={t.avatar_url} size={26} demo={t.is_demo} />
                <div className="min-w-0 flex-1">
                  <Link to={`/players/${t.steam_id}`} className="block truncate text-[13px] font-medium hover:text-teal">
                    {t.persona_name ?? t.steam_id}
                  </Link>
                  <div className="truncate text-[11px] text-faint">
                    {t.last_state === "online" && t.last_server ? `на ${t.last_server} · ` : ""}
                    успешный опрос: {fmtRelative(t.last_success_at)}
                  </div>
                </div>
                <Button size="iconSm" variant="ghost" onClick={() => notify.mutate({ sid: t.steam_id, v: !t.notify })} aria-label={t.notify ? "Выключить уведомления" : "Включить уведомления"} title="Уведомления по игроку">
                  {t.notify ? <Bell className="size-3.5 text-teal" /> : <BellOff className="size-3.5" />}
                </Button>
                <Switch checked={t.enabled} onChange={(val) => toggle.mutate({ sid: t.steam_id, v: val })} label={`Опрос ${t.persona_name ?? t.steam_id}`} />
                <Button size="iconSm" variant="ghost" onClick={() => remove.mutate(t.steam_id)} aria-label="Убрать из трекера">
                  <Trash2 className="size-3.5" />
                </Button>
              </li>
            ))}
          </ul>
          <div className="border-t border-line px-4 py-2">
            <div className="mb-1 text-[11px] font-medium text-faint uppercase">Здоровье источников</div>
            <div className="flex flex-wrap gap-3 text-xs">
              {(dash.data?.health ?? []).map((h) => (
                <span key={h.provider} className="flex items-center gap-1.5">
                  {PROVIDER_NAMES[h.provider] ?? h.provider} <HealthBadge h={h} />
                </span>
              ))}
              {(dash.data?.health ?? []).length === 0 && <span className="text-muted">Проверок ещё не было</span>}
            </div>
          </div>
        </Card>
        <Card>
          <CardHeader
            title="Журнал событий"
            subtitle="Дубликаты исключены; время по опросам — интервалом"
            actions={
              <Button size="sm" variant="ghost" onClick={() => read.mutate()}>
                <CheckCheck className="size-3.5" /> Прочитано
              </Button>
            }
          />
          <div className="flex flex-wrap gap-2 border-b border-line px-3 py-2">
            <Select value={kind} onChange={(e) => setKind(e.target.value)} aria-label="Тип события" className="h-7 text-xs">
              <option value="">Все типы</option>
              {kinds.map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </Select>
            <Select value={player} onChange={(e) => setPlayer(e.target.value)} aria-label="Игрок" className="h-7 text-xs">
              <option value="">Все игроки</option>
              {list.map((t) => (
                <option key={t.steam_id} value={t.steam_id}>
                  {t.persona_name ?? t.steam_id}
                </option>
              ))}
            </Select>
            <label className="flex items-center gap-1.5 text-xs text-muted">
              <Switch checked={unread} onChange={setUnread} label="Только непрочитанные" /> непрочитанные
            </label>
            <Badge className="ml-auto">{evs.length}</Badge>
          </div>
          {events.isLoading && <LoadingBlock />}
          {events.data && evs.length === 0 && <EmptyState title="Событий нет" text="События появляются при наблюдаемом переходе между двумя последовательными опросами." />}
          <div ref={parent} className="max-h-[560px] overflow-y-auto">
            <ul style={{ height: v.getTotalSize(), position: "relative" }} className="divide-y divide-line">
              {v.getVirtualItems().map((it) => (
                <div key={evs[it.index]!.id} style={{ position: "absolute", top: 0, left: 0, right: 0, transform: `translateY(${it.start}px)` }} ref={v.measureElement} data-index={it.index}>
                  <EventItem e={evs[it.index]!} tz={tz} />
                </div>
              ))}
            </ul>
          </div>
        </Card>
      </div>
    </div>
  );
}
