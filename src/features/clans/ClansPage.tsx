import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Bell, Plus, Trash2, UserMinus, Users } from "lucide-react";
import { api } from "@/lib/api";
import type { ClanReport, SquadReport } from "@/lib/types";
import { fmtDuration, fmtPct } from "@/lib/format";
import { useTz } from "@/app/hooks";
import { Avatar, Badge, Button, Card, CardHeader, Dialog, EmptyState, ErrorState, Field, Input, Kpi, LoadingBlock, Notice, PageHeader, Segmented, Select, Switch, cn } from "@/components/ui";
import { ActivityHeatmap } from "@/components/charts/Heatmap";
import { TypicalOfflineCard } from "@/features/activity/ActivityView";
import { Concurrency, SquadTimeline, squadCells } from "@/features/squads/SquadsPage";

/** Адаптер к компонентам отрядов (они ждут SquadReport). */
function asSquad(r: ClanReport): SquadReport {
  return {
    squad: { id: r.clan.id, name: r.clan.name, color: r.clan.color, notes: null, created_at: r.clan.created_at, updated_at: r.clan.updated_at, member_count: r.clan.member_count },
    members: r.members.map((m) => ({ steam_id: m.bm_id, persona_name: m.name, avatar_url: null, provenance: m.provenance, added_at: m.added_at, is_demo: m.is_demo })),
    coverage: r.members.map((m, i) => ({ steam_id: m.bm_id, known_secs: 0, online_secs: 0, coverage: r.coverage[i] ?? 0 })),
    analysis: r.analysis,
    suggestions: [],
    timezone: r.timezone,
  };
}

export function ClansPage() {
  const { id } = useParams();
  const clans = useQuery({ queryKey: ["clans"], queryFn: api.clans });
  const nav = useNavigate();
  const qc = useQueryClient();
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const create = useMutation({
    mutationFn: () => api.saveClan({ name, color: "#a3e635", notify: true, minGather: 2 }),
    onSuccess: (nid) => (qc.invalidateQueries({ queryKey: ["clans"] }), setCreating(false), setName(""), nav(`/clans/${nid}`)),
    onError: (e) => toast.error(e.message),
  });
  const sel = id ? Number(id) : clans.data?.[0]?.id;
  return (
    <div className="space-y-4">
      <PageHeader
        title="Кланы (BattleMetrics)"
        subtitle="Группы игроков BM: общий онлайн, когда весь клан офлайн, уведомление «клан заходит». Быстрее всего — «Собрать клан» во вкладке «Тиммейты» игрока."
        actions={
          <Button variant="primary" onClick={() => setCreating(true)}>
            <Plus className="size-4" /> Новый клан
          </Button>
        }
      />
      <div className="grid gap-4 lg:grid-cols-[240px_minmax(0,1fr)]">
        <Card className="h-fit">
          <CardHeader title="Список" icon={<Users className="size-4" />} />
          {clans.isLoading && <LoadingBlock rows={3} />}
          {clans.data?.length === 0 && <EmptyState title="Кланов нет" text="Откройте игрока BM → «Тиммейты» → «Собрать клан»." />}
          <ul className="p-1.5">
            {clans.data?.map((c) => (
              <li key={c.id}>
                <Link to={`/clans/${c.id}`} className={cn("flex items-center gap-2 rounded-md px-2.5 py-1.5 text-[13px] hover:bg-raised", sel === c.id && "bg-raised")}>
                  <span className="size-2.5 shrink-0 rounded-full" style={{ background: c.color }} />
                  <span className="min-w-0 flex-1 truncate">{c.name}</span>
                  {c.notify && <Bell className="size-3 text-teal" aria-label="уведомления включены" />}
                  <span className="text-[11px] text-faint num">{c.member_count}</span>
                </Link>
              </li>
            ))}
          </ul>
        </Card>
        {sel ? <ClanDetail id={sel} onDeleted={() => nav("/clans")} /> : <Card><EmptyState title="Выберите или создайте клан" /></Card>}
      </div>
      <Dialog
        open={creating}
        onClose={() => setCreating(false)}
        title="Новый клан"
        footer={
          <>
            <Button onClick={() => setCreating(false)}>Отмена</Button>
            <Button variant="primary" disabled={!name.trim()} loading={create.isPending} onClick={() => create.mutate()}>
              Создать
            </Button>
          </>
        }
      >
        <Field label="Название" htmlFor="clan-name">
          <Input id="clan-name" value={name} onChange={(e) => setName(e.target.value)} maxLength={64} autoFocus />
        </Field>
      </Dialog>
    </div>
  );
}

function ClanDetail({ id, onDeleted }: { id: number; onDeleted: () => void }) {
  const tz = useTz();
  const qc = useQueryClient();
  const [preset, setPreset] = useState<"7d" | "30d" | "90d">("30d");
  const q = useQuery({ queryKey: ["clan", id, preset, tz], queryFn: () => api.clanReport(id, { preset }, tz), refetchInterval: 60_000 });
  const bmPlayers = useQuery({ queryKey: ["bm-players", "", "all"], queryFn: () => api.bmPlayers() });
  const inv = () => (qc.invalidateQueries({ queryKey: ["clan", id] }), qc.invalidateQueries({ queryKey: ["clans"] }));
  const save = useMutation({
    mutationFn: (p: { notify: boolean; minGather: number }) => api.saveClan({ id, name: q.data!.clan.name, color: q.data!.clan.color, ...p }),
    onSuccess: inv,
    onError: (e) => toast.error(e.message),
  });
  const add = useMutation({ mutationFn: (bm: string) => api.addClanMember(id, bm), onSuccess: inv, onError: (e) => toast.error(e.message) });
  const remove = useMutation({ mutationFn: (bm: string) => api.removeClanMember(id, bm), onSuccess: inv });
  const del = useMutation({ mutationFn: () => api.deleteClan(id), onSuccess: () => (qc.invalidateQueries({ queryKey: ["clans"] }), onDeleted()) });
  const [confirm, setConfirm] = useState(false);
  if (q.isLoading) return <LoadingBlock rows={8} />;
  if (q.error) return <ErrorState error={q.error} retry={() => q.refetch()} />;
  const r = q.data!;
  const a = r.analysis;
  const sq = asSquad(r);
  const total = a ? a.some_online_secs + a.all_offline_secs + a.unknown_secs : 0;
  const ids = new Set(r.members.map((m) => m.bm_id));
  return (
    <div className="min-w-0 space-y-4">
      <Card className="p-4">
        <div className="flex flex-wrap items-center gap-3">
          <span className="size-3.5 rounded-full" style={{ background: r.clan.color }} />
          <h2 className="font-display text-lg font-semibold">{r.clan.name}</h2>
          <Badge tone={r.online_now ? "lime" : "neutral"}>онлайн сейчас: {r.online_now}</Badge>
          <div className="ml-auto flex flex-wrap items-center gap-3">
            <label className="flex items-center gap-2 text-xs text-muted" title="Уведомление, когда несколько участников заходят почти одновременно">
              «Клан заходит»
              <Switch checked={r.clan.notify} onChange={(v) => save.mutate({ notify: v, minGather: r.clan.min_gather })} label="Уведомлять, когда клан заходит" />
            </label>
            <label className="flex items-center gap-1.5 text-xs text-muted">
              от
              <Select value={r.clan.min_gather} onChange={(e) => save.mutate({ notify: r.clan.notify, minGather: Number(e.target.value) })} aria-label="Сколько участников" className="h-7 text-xs">
                {[2, 3, 4, 5].map((n) => (
                  <option key={n} value={n}>
                    {n}
                  </option>
                ))}
              </Select>
              за 10 мин
            </label>
            <Segmented label="Период" value={preset} onChange={setPreset} options={[{ value: "7d", label: "7 д" }, { value: "30d", label: "30 д" }, { value: "90d", label: "90 д" }]} />
            <Button size="icon" variant="ghost" aria-label="Удалить клан" onClick={() => setConfirm(true)}>
              <Trash2 className="size-4" />
            </Button>
          </div>
        </div>
      </Card>
      <Card>
        <CardHeader
          title="Участники"
          actions={
            <Select value="" onChange={(e) => e.target.value && add.mutate(e.target.value)} aria-label="Добавить участника" className="h-7 max-w-[220px] text-xs">
              <option value="">+ добавить игрока BM…</option>
              {(bmPlayers.data ?? []).filter((p) => !ids.has(p.bm_id)).slice(0, 300).map((p) => (
                <option key={p.bm_id} value={p.bm_id}>
                  {p.name ?? p.bm_id}
                </option>
              ))}
            </Select>
          }
        />
        <ul className="divide-y divide-line">
          {r.members.map((m, i) => (
            <li key={m.bm_id} className="flex items-center gap-3 px-4 py-2">
              <Avatar name={m.name} size={24} demo={m.is_demo} />
              <Link to={`/bm/${m.bm_id}`} className="min-w-0 flex-1 truncate text-[13px] font-medium hover:text-teal">
                {m.name ?? m.bm_id}
              </Link>
              {m.online_server && <Badge tone="lime">онлайн · {m.online_server}</Badge>}
              <Badge>{{ manual: "вручную", teammate_suggestion: "из тиммейтов", demo: "демо" }[m.provenance] ?? m.provenance}</Badge>
              <span className="w-24 text-right text-[11px] text-faint num">покрытие {fmtPct(r.coverage[i] ?? 0)}</span>
              <Button size="iconSm" variant="ghost" aria-label="Убрать из клана" onClick={() => remove.mutate(m.bm_id)}>
                <UserMinus className="size-3.5" />
              </Button>
            </li>
          ))}
        </ul>
      </Card>
      {a && (
        <>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Kpi label="Кто-то онлайн" value={fmtPct(total ? a.some_online_secs / total : null)} sub={fmtDuration(a.some_online_secs)} tone="lime" />
            <Kpi label="Весь клан офлайн" value={fmtPct(total ? a.all_offline_secs / total : null)} sub="при наблюдении за всеми" />
            <Kpi label="Неизвестно" value={fmtPct(total ? a.unknown_secs / total : null)} tone="amber" />
            <Kpi label="Все вместе онлайн" value={fmtDuration(a.concurrency_secs[a.members] ?? 0)} />
          </div>
          <Card>
            <CardHeader title="Совместная шкала" subtitle="Последние 7 дней периода" />
            <div className="p-4">
              <SquadTimeline r={sq} tz={tz} />
            </div>
          </Card>
          <div className="grid gap-4 xl:grid-cols-[1.4fr_1fr]">
            <Card>
              <CardHeader title="Когда кто-то из клана онлайн" subtitle="Жёлтая рамка — типичные часы, когда весь клан офлайн" />
              <div className="p-4">
                <ActivityHeatmap cells={squadCells(sq)} highlight={new Set(a.typical_offline.status === "available" ? a.typical_offline.hours.map(([w, h]) => `${w}:${h}`) : [])} />
              </div>
            </Card>
            <Card>
              <CardHeader title="Одновременно онлайн" />
              <div className="px-2 pb-2">
                <Concurrency secs={a.concurrency_secs} />
              </div>
            </Card>
          </div>
          <TypicalOfflineCard t={a.typical_offline} squad />
        </>
      )}
      <Notice tone="info">Наблюдение и уведомления «клан заходит» работают, пока программа запущена. Данные берутся из наблюдаемых серверов и истории BattleMetrics.</Notice>
      <Dialog
        open={confirm}
        onClose={() => setConfirm(false)}
        title="Удалить клан?"
        footer={
          <>
            <Button onClick={() => setConfirm(false)}>Отмена</Button>
            <Button variant="danger" onClick={() => del.mutate()} loading={del.isPending}>
              Удалить
            </Button>
          </>
        }
      >
        <p className="text-[13px] text-muted">Игроки и их сессии останутся; удаляется только группа.</p>
      </Dialog>
    </div>
  );
}
