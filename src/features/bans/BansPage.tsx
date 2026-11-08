import { useMemo, useState } from "react";
import { Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import type { Ban } from "@/lib/types";
import { Avatar, Badge, Card, EmptyState, ErrorState, Input, LoadingBlock, Notice, PageHeader, Segmented } from "@/components/ui";

function cell(bans: Ban[], kind: Ban["kind"]) {
  const list = bans.filter((b) => b.kind === kind);
  if (!list.length) return <span className="text-faint">нет данных</span>;
  const active = list.filter((b) => b.active);
  if (!active.length) return <span className="text-muted">нет</span>;
  const count = active.reduce((s, b) => s + (b.count ?? 1), 0);
  return (
    <Badge tone="danger" title={active.map((b) => b.project_name || b.reason || "").filter(Boolean).join("; ") || undefined}>
      {count}
      {active[0]?.days_since_last !== null && active[0]?.days_since_last !== undefined ? ` · ${active[0].days_since_last} дн.` : ""}
    </Badge>
  );
}

export function BansPage() {
  const q = useQuery({ queryKey: ["bans"], queryFn: api.bans });
  const [filter, setFilter] = useState<"all" | "banned">("all");
  const [text, setText] = useState("");
  const rows = useMemo(
    () =>
      (q.data ?? []).filter((r) => (filter === "banned" ? r.bans.some((b) => b.active) : true) && (!text || r.player.persona_name?.toLowerCase().includes(text.toLowerCase()) || r.player.steam_id.includes(text))),
    [q.data, filter, text],
  );
  return (
    <div className="space-y-4">
      <PageHeader title="Баны и риски" subtitle="Steam-баны и баны проектов — раздельно. Индикаторы риска объяснимы и не являются вердиктом." />
      <Notice tone="info">Steam Game Ban не указывает конкретную игру и не обязательно означает бан в Rust/EAC. Баны проектов показываются только из документированных источников.</Notice>
      <div className="flex flex-wrap gap-2">
        <Input value={text} onChange={(e) => setText(e.target.value)} placeholder="Фильтр" className="max-w-xs" aria-label="Фильтр" />
        <Segmented label="Показывать" value={filter} onChange={setFilter} options={[{ value: "all", label: "Все" }, { value: "banned", label: "С банами" }]} />
      </div>
      <Card>
        {q.isLoading && <LoadingBlock />}
        {q.error && <ErrorState error={q.error} retry={() => q.refetch()} />}
        {q.data && rows.length === 0 && <EmptyState title="Нет игроков" />}
        {rows.length > 0 && (
          <div className="overflow-x-auto">
            <table className="w-full min-w-[820px] text-[13px]">
              <thead className="text-left text-[11px] tracking-wide text-faint uppercase">
                <tr>
                  <th className="px-4 py-2 font-medium">Игрок</th>
                  <th className="px-2 py-2 font-medium">VAC</th>
                  <th className="px-2 py-2 font-medium">Game</th>
                  <th className="px-2 py-2 font-medium">Community</th>
                  <th className="px-2 py-2 font-medium">Economy</th>
                  <th className="px-2 py-2 font-medium">Проекты</th>
                  <th className="px-4 py-2 text-right font-medium">Индикаторы</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {rows.map((r) => (
                  <tr key={r.player.steam_id} className="hover:bg-raised">
                    <td className="px-4 py-2">
                      <Link to={`/players/${r.player.steam_id}`} className="flex items-center gap-2 hover:text-teal">
                        <Avatar name={r.player.persona_name} url={r.player.avatar_url} size={22} demo={r.player.is_demo} />
                        {r.player.persona_name ?? r.player.steam_id}
                      </Link>
                    </td>
                    <td className="px-2 py-2">{cell(r.bans, "vac")}</td>
                    <td className="px-2 py-2">{cell(r.bans, "game")}</td>
                    <td className="px-2 py-2">{cell(r.bans, "community")}</td>
                    <td className="px-2 py-2">{cell(r.bans, "economy")}</td>
                    <td className="px-2 py-2">{r.bans.some((b) => b.kind === "project") ? cell(r.bans, "project") : <span className="text-faint">не подключено</span>}</td>
                    <td className="px-4 py-2 text-right num" title={r.risk.indicators.filter((i) => i.triggered).map((i) => `${i.label} (+${i.weight})`).join("\n") || "Ни один индикатор не сработал"}>
                      {r.risk.enabled ? `${(r.risk.score + 0).toFixed(2)} / ${r.risk.max_score.toFixed(2)}` : "выкл."}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>
    </div>
  );
}
