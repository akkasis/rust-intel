import { useMemo, useRef, useState } from "react";
import { Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDownUp, Star } from "lucide-react";
import { api } from "@/lib/api";
import type { Player } from "@/lib/types";
import { fmtRelative } from "@/lib/format";
import { Avatar, Badge, Card, EmptyState, ErrorState, Input, LoadingBlock, PageHeader, Segmented, StatusBadge } from "@/components/ui";

type SortKey = "name" | "hours" | "updated";

export function PlayersPage() {
  const [q, setQ] = useState("");
  const [fav, setFav] = useState<"all" | "fav">("all");
  const [sort, setSort] = useState<SortKey>("updated");
  const players = useQuery({ queryKey: ["players", ""], queryFn: () => api.players() });
  const rows = useMemo(() => {
    const l = q.trim().toLowerCase();
    const list = (players.data ?? []).filter((p) => (fav === "fav" ? p.favorite : true) && (!l || p.persona_name?.toLowerCase().includes(l) || p.steam_id.includes(l)));
    const cmp: Record<SortKey, (a: Player, b: Player) => number> = {
      name: (a, b) => (a.persona_name ?? "").localeCompare(b.persona_name ?? "", "ru"),
      hours: (a, b) => (b.rust_hours ?? -1) - (a.rust_hours ?? -1),
      updated: (a, b) => b.updated_at.localeCompare(a.updated_at),
    };
    return [...list].sort(cmp[sort]);
  }, [players.data, q, fav, sort]);
  const parent = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({ count: rows.length, getScrollElement: () => parent.current, estimateSize: () => 44, overscan: 10 });

  return (
    <div className="space-y-4">
      <PageHeader title="Игроки" subtitle="Все профили в локальной базе" />
      <div className="flex flex-wrap items-center gap-2">
        <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Фильтр по нику или SteamID" className="max-w-xs" aria-label="Фильтр" />
        <Segmented label="Фильтр избранного" value={fav} onChange={setFav} options={[{ value: "all", label: "Все" }, { value: "fav", label: "Избранные" }]} />
        <div className="ml-auto flex items-center gap-1.5 text-xs text-muted">
          <ArrowDownUp className="size-3.5" />
          <Segmented
            label="Сортировка"
            value={sort}
            onChange={setSort}
            options={[
              { value: "updated", label: "Обновлены" },
              { value: "name", label: "Ник" },
              { value: "hours", label: "Часы" },
            ]}
          />
        </div>
      </div>
      <Card>
        <div className="grid grid-cols-[minmax(0,2fr)_minmax(0,1.3fr)_90px_110px_130px] gap-2 border-b border-line px-4 py-2 text-[11px] font-medium tracking-wide text-faint uppercase">
          <span>Игрок</span>
          <span>SteamID64</span>
          <span className="text-right">Часы Rust</span>
          <span>Steam</span>
          <span>Обновлён</span>
        </div>
        {players.isLoading && <LoadingBlock />}
        {players.error && <ErrorState error={players.error} retry={() => players.refetch()} />}
        {players.data && rows.length === 0 && <EmptyState title="Нет игроков" text="Найдите игрока через «Поиск игрока»." />}
        <div ref={parent} className="max-h-[calc(100vh-260px)] overflow-y-auto" role="table" aria-label="Список игроков" aria-rowcount={rows.length}>
          <div style={{ height: v.getTotalSize(), position: "relative" }}>
            {v.getVirtualItems().map((it) => {
              const p = rows[it.index]!;
              return (
                <Link
                  key={p.steam_id}
                  to={`/players/${p.steam_id}`}
                  role="row"
                  style={{ position: "absolute", top: 0, left: 0, right: 0, transform: `translateY(${it.start}px)`, height: it.size }}
                  className="grid grid-cols-[minmax(0,2fr)_minmax(0,1.3fr)_90px_110px_130px] items-center gap-2 border-b border-line px-4 hover:bg-raised"
                >
                  <span className="flex min-w-0 items-center gap-2">
                    <Avatar name={p.persona_name} url={p.avatar_url} size={26} demo={p.is_demo} />
                    <span className="truncate font-medium">{p.persona_name ?? "—"}</span>
                    {p.favorite && <Star className="size-3.5 shrink-0 fill-amber text-amber" aria-label="в избранном" />}
                    {p.steam_in_game && <Badge tone="lime">в Rust</Badge>}
                  </span>
                  <span className="truncate text-xs text-muted num">{p.steam_id}</span>
                  <span className="text-right text-xs num">{p.rust_hours !== null ? Math.round(p.rust_hours) : <span className="text-faint">скрыто</span>}</span>
                  <span>
                    <StatusBadge status={p.is_demo ? "demo" : (p.steam_status as never)} />
                  </span>
                  <span className="text-xs text-faint">{fmtRelative(p.steam_fetched_at)}</span>
                </Link>
              );
            })}
          </div>
        </div>
      </Card>
    </div>
  );
}
