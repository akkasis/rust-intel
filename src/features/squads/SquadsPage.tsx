import { useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Pencil, Plus, Trash2, UserMinus, Users } from "lucide-react";
import { api } from "@/lib/api";
import type { HeatCell, SquadReport, SquadSegment } from "@/lib/types";
import { fmtDateTime, fmtDuration, fmtPct, fmtRelative } from "@/lib/format";
import { useTz } from "@/app/hooks";
import { Avatar, Badge, Button, Card, CardHeader, Dialog, EmptyState, ErrorState, Field, Input, Kpi, LoadingBlock, Notice, PageHeader, Segmented, Select, cn } from "@/components/ui";
import { ActivityHeatmap } from "@/components/charts/Heatmap";
import { EChart, baseOption, chartTheme } from "@/components/charts/EChart";
import { TypicalOfflineCard } from "@/features/activity/ActivityView";

const COLORS = ["#2dd4bf", "#a3e635", "#f2a93b", "#a78bfa", "#f0525c", "#38bdf8"];

export function SquadsPage() {
  const { id } = useParams();
  const squads = useQuery({ queryKey: ["squads"], queryFn: api.squads });
  const [edit, setEdit] = useState<{ id?: number; name: string; color: string; notes: string } | null>(null);
  const nav = useNavigate();
  const sel = id ? Number(id) : squads.data?.[0]?.id;
  return (
    <div className="space-y-4">
      <PageHeader
        title="Отряды"
        subtitle="Вручную заданные группы игроков: совместное присутствие, пересечения, часы без наблюдаемого онлайна"
        actions={
          <Button variant="primary" onClick={() => setEdit({ name: "", color: COLORS[0]!, notes: "" })}>
            <Plus className="size-4" /> Новый отряд
          </Button>
        }
      />
      <div className="grid gap-4 lg:grid-cols-[240px_minmax(0,1fr)]">
        <Card className="h-fit">
          <CardHeader title="Список" icon={<Users className="size-4" />} />
          {squads.isLoading && <LoadingBlock rows={3} />}
          {squads.data?.length === 0 && <EmptyState title="Отрядов нет" text="Создайте отряд и добавьте 1–20 игроков." />}
          <ul className="p-1.5">
            {squads.data?.map((s) => (
              <li key={s.id}>
                <Link to={`/squads/${s.id}`} className={cn("flex items-center gap-2 rounded-md px-2.5 py-1.5 text-[13px] hover:bg-raised", sel === s.id && "bg-raised")}>
                  <span className="size-2.5 shrink-0 rounded-full" style={{ background: s.color }} />
                  <span className="min-w-0 flex-1 truncate">{s.name}</span>
                  <span className="text-[11px] text-faint num">{s.member_count}</span>
                </Link>
              </li>
            ))}
          </ul>
        </Card>
        {sel ? <SquadDetail id={sel} onEdit={(s) => setEdit(s)} onDeleted={() => nav("/squads")} /> : <Card><EmptyState title="Выберите или создайте отряд" /></Card>}
      </div>
      <SquadEditor value={edit} onClose={() => setEdit(null)} onSaved={(nid) => nav(`/squads/${nid}`)} />
    </div>
  );
}

function SquadEditor({ value, onClose, onSaved }: { value: { id?: number; name: string; color: string; notes: string } | null; onClose: () => void; onSaved: (id: number) => void }) {
  const qc = useQueryClient();
  const [v, setV] = useState(value);
  const [prev, setPrev] = useState(value);
  if (value !== prev) {
    setPrev(value);
    setV(value);
  }
  const save = useMutation({
    mutationFn: () => api.saveSquad({ id: v?.id ?? null, name: v!.name, color: v!.color, notes: v!.notes || null }),
    onSuccess: (id) => {
      qc.invalidateQueries({ queryKey: ["squads"] });
      qc.invalidateQueries({ queryKey: ["squad"] });
      toast.success("Отряд сохранён");
      onClose();
      onSaved(id);
    },
    onError: (e) => toast.error(e.message),
  });
  return (
    <Dialog
      open={!!value}
      onClose={onClose}
      title={v?.id ? "Редактировать отряд" : "Новый отряд"}
      footer={
        <>
          <Button onClick={onClose}>Отмена</Button>
          <Button variant="primary" loading={save.isPending} disabled={!v?.name.trim()} onClick={() => save.mutate()}>
            Сохранить
          </Button>
        </>
      }
    >
      {v && (
        <div className="space-y-3">
          <Field label="Название" htmlFor="squad-name">
            <Input id="squad-name" value={v.name} maxLength={64} onChange={(e) => setV({ ...v, name: e.target.value })} autoFocus />
          </Field>
          <Field label="Цвет">
            <div className="flex gap-2">
              {COLORS.map((c) => (
                <button key={c} type="button" onClick={() => setV({ ...v, color: c })} className={cn("size-6 rounded-full border-2", v.color === c ? "border-fg" : "border-transparent")} style={{ background: c }} aria-label={`Цвет ${c}`} />
              ))}
            </div>
          </Field>
          <Field label="Заметка">
            <Input value={v.notes} maxLength={1000} onChange={(e) => setV({ ...v, notes: e.target.value })} />
          </Field>
        </div>
      )}
    </Dialog>
  );
}

function SquadDetail({ id, onEdit, onDeleted }: { id: number; onEdit: (s: { id: number; name: string; color: string; notes: string }) => void; onDeleted: () => void }) {
  const tz = useTz();
  const qc = useQueryClient();
  const [preset, setPreset] = useState<"7d" | "30d" | "90d">("30d");
  const q = useQuery({ queryKey: ["squad", id, preset, tz], queryFn: () => api.squadReport(id, { preset }, tz) });
  const players = useQuery({ queryKey: ["players", ""], queryFn: () => api.players() });
  const [confirm, setConfirm] = useState(false);
  const inv = () => {
    qc.invalidateQueries({ queryKey: ["squad", id] });
    qc.invalidateQueries({ queryKey: ["squads"] });
  };
  const add = useMutation({ mutationFn: (sid: string) => api.addSquadMember(id, sid), onSuccess: inv, onError: (e) => toast.error(e.message) });
  const accept = useMutation({ mutationFn: (sid: string) => api.addSquadMember(id, sid, "accepted_suggestion"), onSuccess: inv, onError: (e) => toast.error(e.message) });
  const remove = useMutation({ mutationFn: (sid: string) => api.removeSquadMember(id, sid), onSuccess: inv });
  const del = useMutation({
    mutationFn: () => api.deleteSquad(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["squads"] });
      setConfirm(false);
      onDeleted();
    },
  });
  if (q.isLoading) return <LoadingBlock rows={8} />;
  if (q.error) return <ErrorState error={q.error} retry={() => q.refetch()} />;
  const r = q.data!;
  const a = r.analysis;
  const memberIds = new Set(r.members.map((m) => m.steam_id));
  const total = a ? a.some_online_secs + a.all_offline_secs + a.unknown_secs : 0;
  return (
    <div className="min-w-0 space-y-4">
      <Card className="p-4">
        <div className="flex flex-wrap items-center gap-3">
          <span className="size-3.5 rounded-full" style={{ background: r.squad.color }} />
          <h2 className="font-display text-lg font-semibold">{r.squad.name}</h2>
          <Badge>{r.members.length} / 20</Badge>
          <div className="ml-auto flex items-center gap-2">
            <Segmented label="Период" value={preset} onChange={setPreset} options={[{ value: "7d", label: "7 дней" }, { value: "30d", label: "30 дней" }, { value: "90d", label: "90 дней" }]} />
            <Button size="icon" variant="ghost" aria-label="Редактировать отряд" onClick={() => onEdit({ id, name: r.squad.name, color: r.squad.color, notes: r.squad.notes ?? "" })}>
              <Pencil className="size-4" />
            </Button>
            <Button size="icon" variant="ghost" aria-label="Удалить отряд" onClick={() => setConfirm(true)}>
              <Trash2 className="size-4" />
            </Button>
          </div>
        </div>
        {r.squad.notes && <p className="mt-1 text-xs text-muted">{r.squad.notes}</p>}
      </Card>

      <Card>
        <CardHeader
          title="Участники"
          subtitle="Покрытие — доля периода, когда источник наблюдал участника"
          actions={
            <Select value="" onChange={(e) => e.target.value && add.mutate(e.target.value)} aria-label="Добавить участника" className="h-7 text-xs" disabled={r.members.length >= 20}>
              <option value="">+ добавить игрока…</option>
              {(players.data ?? [])
                .filter((p) => !memberIds.has(p.steam_id))
                .map((p) => (
                  <option key={p.steam_id} value={p.steam_id}>
                    {p.persona_name ?? p.steam_id}
                  </option>
                ))}
            </Select>
          }
        />
        {r.members.length === 0 ? (
          <EmptyState title="В отряде никого" text="Добавьте игроков из локальной базы." />
        ) : (
          <ul className="divide-y divide-line">
            {r.members.map((m, i) => {
              const c = r.coverage[i];
              return (
                <li key={m.steam_id} className="flex items-center gap-3 px-4 py-2">
                  <Avatar name={m.persona_name} url={m.avatar_url} size={26} demo={m.is_demo} />
                  <Link to={`/players/${m.steam_id}`} className="min-w-0 flex-1 truncate text-[13px] font-medium hover:text-teal">
                    {m.persona_name ?? m.steam_id}
                  </Link>
                  <Badge title={`Добавлен ${fmtRelative(m.added_at)}`}>{{ manual: "вручную", accepted_suggestion: "из предложений", demo: "демо" }[m.provenance] ?? m.provenance}</Badge>
                  <div className="w-36" title={c ? `Наблюдалось ${fmtDuration(c.known_secs)}, онлайн ${fmtDuration(c.online_secs)}` : undefined}>
                    <div className="h-1.5 overflow-hidden rounded bg-raised">
                      <div className={cn("h-full", (c?.coverage ?? 0) < 0.3 ? "bg-amber" : "bg-teal")} style={{ width: `${Math.round((c?.coverage ?? 0) * 100)}%` }} />
                    </div>
                    <div className="mt-0.5 text-right text-[10px] text-faint num">покрытие {fmtPct(c?.coverage ?? 0)}</div>
                  </div>
                  <Button size="iconSm" variant="ghost" aria-label="Убрать из отряда" onClick={() => remove.mutate(m.steam_id)}>
                    <UserMinus className="size-3.5" />
                  </Button>
                </li>
              );
            })}
          </ul>
        )}
      </Card>

      {a && (
        <>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Kpi label="Кто-то онлайн" value={fmtPct(total ? a.some_online_secs / total : null)} sub={fmtDuration(a.some_online_secs)} tone="lime" />
            <Kpi label="Все наблюдались офлайн" value={fmtPct(total ? a.all_offline_secs / total : null)} sub={fmtDuration(a.all_offline_secs)} hint="Только когда покрытие есть у КАЖДОГО участника" />
            <Kpi label="Неизвестно" value={fmtPct(total ? a.unknown_secs / total : null)} sub="нет покрытия хотя бы у одного" tone="amber" />
            <Kpi label="Все вместе онлайн" value={fmtDuration(a.concurrency_secs[a.members] ?? 0)} sub={`${a.members} из ${a.members}`} />
          </div>
          <Card>
            <CardHeader title="Совместная временная шкала" subtitle="Последние 7 дней периода · штриховка — нет наблюдений" />
            <div className="p-4">
              <SquadTimeline r={r} tz={tz} />
            </div>
          </Card>
          <div className="grid gap-4 xl:grid-cols-[1.4fr_1fr]">
            <Card>
              <CardHeader title="Когда кто-то из отряда наблюдается онлайн" subtitle="Доля «кто-то онлайн» от времени, где состояние отряда известно" />
              <div className="p-4">
                <ActivityHeatmap cells={squadCells(r)} highlight={new Set(a.typical_offline.status === "available" ? a.typical_offline.hours.map(([w, h]) => `${w}:${h}`) : [])} />
              </div>
            </Card>
            <div className="space-y-4">
              <Card>
                <CardHeader title="Одновременно онлайн" subtitle="Время с ровно k участниками (только известные интервалы)" />
                <div className="px-2 pb-2">
                  <Concurrency secs={a.concurrency_secs} />
                </div>
              </Card>
              <Card>
                <CardHeader title="Пересечения пар" />
                <ul className="max-h-56 divide-y divide-line overflow-y-auto">
                  {a.pairs
                    .slice()
                    .sort((x, y) => y.co_online_secs - x.co_online_secs)
                    .map((p) => (
                      <li key={`${p.a}-${p.b}`} className="flex justify-between gap-2 px-4 py-1.5 text-[13px]">
                        <span className="truncate">
                          {r.members[p.a]?.persona_name} + {r.members[p.b]?.persona_name}
                        </span>
                        <span className="text-muted num">{fmtDuration(p.co_online_secs)}</span>
                      </li>
                    ))}
                  {a.pairs.length === 0 && <li className="px-4 py-3 text-xs text-muted">Нужно минимум 2 участника</li>}
                </ul>
              </Card>
            </div>
          </div>
          <TypicalOfflineCard t={a.typical_offline} squad />
        </>
      )}

      <Card>
        <CardHeader title="Возможные тиммейты" subtitle="Гипотезы: ≥ 3 совместные сессии на одном сервере (≥ 20 мин) с участниками. Не утверждение." />
        {r.suggestions.length === 0 ? (
          <EmptyState title="Предложений нет" text="Учитываются только игроки в локальной базе с наблюдаемыми серверными сессиями." />
        ) : (
          <ul className="divide-y divide-line">
            {r.suggestions.map((s) => (
              <li key={s.steam_id} className="px-4 py-2">
                <div className="flex items-center gap-2">
                  <Link to={`/players/${s.steam_id}`} className="font-medium hover:text-teal">
                    {s.persona_name ?? s.steam_id}
                  </Link>
                  <Badge>{s.co_sessions} совместных</Badge>
                  <span className="text-[11px] text-faint">с: {s.with_members.join(", ")}</span>
                  <Button size="sm" className="ml-auto" onClick={() => accept.mutate(s.steam_id)}>
                    Добавить в отряд
                  </Button>
                </div>
                <ul className="mt-1 space-y-0.5 text-[11px] text-faint">
                  {s.evidence.map((e, i) => (
                    <li key={i} className="num">
                      {fmtDateTime(e.start, tz)} · {fmtDuration(e.overlap_secs)} · {e.server_id}
                    </li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>
        )}
      </Card>
      <Notice tone="info">«Все наблюдались офлайн» засчитывается только при покрытии каждого участника. Пропуск данных у любого — «неизвестно», не «офлайн».</Notice>
      <Dialog
        open={confirm}
        onClose={() => setConfirm(false)}
        title="Удалить отряд?"
        footer={
          <>
            <Button onClick={() => setConfirm(false)}>Отмена</Button>
            <Button variant="danger" loading={del.isPending} onClick={() => del.mutate()}>
              Удалить
            </Button>
          </>
        }
      >
        <p className="text-[13px] text-muted">Игроки и их история останутся в базе; удаляется только группа.</p>
      </Dialog>
    </div>
  );
}

export function squadCells(r: SquadReport): HeatCell[] {
  return (r.analysis?.hours ?? []).map((h) => {
    const known = h.some_online_secs + h.all_offline_secs;
    return {
      weekday: h.weekday,
      hour: h.hour,
      online_secs: h.some_online_secs,
      known_secs: known,
      sample_days: h.sample_days,
      ratio: h.all_offline_ratio === null ? null : 1 - h.all_offline_ratio,
      state: known === 0 ? "unknown" : h.all_offline_ratio === null ? "insufficient" : "known",
    };
  });
}

export function Concurrency({ secs }: { secs: number[] }) {
  const option = useMemo(() => {
    const t = chartTheme();
    return {
      ...baseOption(),
      grid: { left: 40, right: 8, top: 10, bottom: 24 },
      xAxis: { type: "category", data: secs.map((_, i) => String(i)), name: "онлайн", nameLocation: "middle", nameGap: 18, axisLine: { lineStyle: { color: t.line } }, axisTick: { show: false } },
      yAxis: { type: "value", axisLabel: { formatter: (v: number) => `${Math.round(v / 3600)}ч` }, splitLine: { lineStyle: { color: t.line } } },
      tooltip: { ...(baseOption().tooltip as object), valueFormatter: (v: number) => fmtDuration(v) },
      series: [{ type: "bar", data: secs.map((v, i) => ({ value: v, itemStyle: { color: i === 0 ? t.faint : t.lime } })), barMaxWidth: 26, itemStyle: { borderRadius: [3, 3, 0, 0] } }],
    };
  }, [secs]);
  return <EChart option={option} height={170} label="Распределение времени по числу одновременно онлайн участников" />;
}

export function SquadTimeline({ r, tz }: { r: SquadReport; tz: string }) {
  const a = r.analysis!;
  const end = new Date(a.range_end).getTime();
  const start = Math.max(new Date(a.range_start).getTime(), end - 7 * 86_400_000);
  const W = 1000;
  const rowH = 16;
  const rows = r.members.length + 1;
  const x = (t: number) => ((t - start) / (end - start)) * W;
  const [hover, setHover] = useState<string>("");
  const segs = a.segments.filter((s) => new Date(s.end).getTime() > start);
  const days: number[] = [];
  for (let d = Math.ceil(start / 86_400_000) * 86_400_000; d < end; d += 86_400_000) days.push(d);
  const bar = (s: SegmentLike, row: number, key: string, label: string, paint: { className?: string; fill?: string }) => {
    const x0 = x(Math.max(new Date(s.start).getTime(), start));
    const x1 = x(new Date(s.end).getTime());
    if (x1 - x0 < 0.2) return null;
    return <rect key={key} x={x0} y={row * (rowH + 6)} width={x1 - x0} height={rowH} rx={1.5} onMouseEnter={() => setHover(label)} {...paint} />;
  };
  const HATCH = { fill: "url(#sq-hatch)" };
  return (
    <div>
      <div className="flex gap-2">
        <div className="w-28 shrink-0 space-y-[6px] pt-0 text-[11px]">
          {r.members.map((m) => (
            <div key={m.steam_id} className="h-4 truncate leading-4 text-muted">
              {m.persona_name}
            </div>
          ))}
          <div className="h-4 truncate leading-4 font-medium text-fg">Отряд</div>
        </div>
        <svg viewBox={`0 0 ${W} ${rows * (rowH + 6) + 14}`} className="w-full" preserveAspectRatio="none" role="img" aria-label="Временная шкала присутствия участников отряда">
          <defs>
            <pattern id="sq-hatch" width="5" height="5" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
              <rect width="5" height="5" fill="var(--rid-unknown)" />
              <line x1="0" y1="0" x2="0" y2="5" stroke="var(--rid-hatch)" strokeWidth="1.5" />
            </pattern>
          </defs>
          {days.map((d) => (
            <g key={d}>
              <line x1={x(d)} x2={x(d)} y1={0} y2={rows * (rowH + 6)} stroke="var(--rid-line)" />
              <text x={x(d) + 3} y={rows * (rowH + 6) + 11} fontSize="10" fill="var(--rid-faint)">
                {fmtDateTime(new Date(d).toISOString(), tz, { hour: undefined, minute: undefined })}
              </text>
            </g>
          ))}
          {r.members.map((m, i) =>
            segs.map((s, k) => {
              const bit = 1 << i;
              const on = (s.online_mask & bit) !== 0;
              const known = (s.known_mask & bit) !== 0;
              const label = `${m.persona_name}: ${on ? "онлайн" : known ? "наблюдался офлайн" : "нет наблюдений"} · ${fmtDateTime(s.start, tz)}–${fmtDateTime(s.end, tz)}`;
              if (on) return bar(s, i, `${i}-${k}`, label, { className: "fill-lime/85" });
              if (!known) return bar(s, i, `${i}-${k}`, label, HATCH);
              return bar(s, i, `${i}-${k}`, label, { className: "fill-raised" });
            }),
          )}
          {segs.map((s, k) => {
            const label = `Отряд: ${s.state === "some_online" ? `онлайн ${s.online_count}` : s.state === "all_observed_offline" ? "все наблюдались офлайн" : "неизвестно"} · ${fmtDateTime(s.start, tz)}–${fmtDateTime(s.end, tz)}`;
            if (s.state === "unknown") return bar(s, r.members.length, `a${k}`, label, HATCH);
            return bar(s, r.members.length, `a${k}`, label, { className: s.state === "some_online" ? "fill-teal" : "fill-line-strong" });
          })}
        </svg>
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-4 text-[11px] text-muted">
        <span className="flex items-center gap-1"><span className="inline-block size-2.5 rounded-sm bg-lime" /> онлайн</span>
        <span className="flex items-center gap-1"><span className="inline-block size-2.5 rounded-sm bg-raised border border-line" /> наблюдался офлайн</span>
        <span className="flex items-center gap-1"><span className="hatch inline-block size-2.5 rounded-sm" /> нет наблюдений</span>
        <span className="ml-auto min-h-4 num" aria-live="polite">{hover || "Наведите на полосу"}</span>
      </div>
    </div>
  );
}

type SegmentLike = Pick<SquadSegment, "start" | "end">;
