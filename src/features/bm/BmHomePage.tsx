import { useState } from "react";
import { Link, useNavigate } from "react-router";
import { useMutation, useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { ExternalLink, Search, Server } from "lucide-react";
import { api } from "@/lib/api";
import { fmtRelative } from "@/lib/format";
import { openExternal } from "@/lib/external";
import { useIsDemo } from "@/app/hooks";
import { Avatar, Badge, Button, Card, CardHeader, EmptyState, Input, LoadingBlock, Notice, PageHeader, Segmented } from "@/components/ui";

/** Ссылка/ID игрока BM → id, иначе null. */
export function bmIdFrom(input: string): string | null {
  const t = input.trim().replace(/\/$/, "");
  if (/^\d{1,15}$/.test(t)) return t;
  const m = /battlemetrics\.com\/(?:rcon\/)?players\/(\d{1,15})/i.exec(t);
  return m ? m[1]! : null;
}

export function BmHomePage() {
  const demo = useIsDemo();
  const nav = useNavigate();
  const [q, setQ] = useState("");
  const [filter, setFilter] = useState("");
  const [tracked, setTracked] = useState<"all" | "tracked">("all");
  const local = useQuery({ queryKey: ["bm-players", filter, tracked], queryFn: () => api.bmPlayers(filter || undefined, tracked === "tracked") });
  const watch = useQuery({ queryKey: ["server-watch"], queryFn: api.serverWatch });
  const search = useMutation({ mutationFn: (name: string) => api.searchBattlemetrics(name), onError: (e) => toast.error(e.message) });
  const go = () => {
    const id = bmIdFrom(q);
    if (id) nav(`/bm/${id}`);
    else if (demo) setFilter(q);
    else search.mutate(q.trim());
  };
  return (
    <div className="space-y-4">
      <PageHeader title="BattleMetrics" subtitle="Любой игрок BM — по нику или ссылке. Серверы, сессии, активность и возможные тиммейты." />
      <Card className="p-4">
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            go();
          }}
        >
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute top-2 left-2.5 size-4 text-faint" />
            <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Ник игрока или ссылка battlemetrics.com/players/…" className="h-9 pl-8 text-sm" aria-label="Поиск игрока BattleMetrics" maxLength={200} />
          </div>
          <Button type="submit" variant="primary" className="h-9 px-4" loading={search.isPending} disabled={q.trim().length < 2}>
            Найти
          </Button>
        </form>
        {demo && <p className="mt-2 text-xs text-muted">Демо: поиск по нику идёт по вымышленным игрокам ниже. В режиме Live — по всей базе BattleMetrics.</p>}
        {search.data && (
          <ul className="mt-3 divide-y divide-line rounded-md border border-line">
            {search.data.length === 0 && <li className="px-3 py-3 text-xs text-muted">Никого не найдено</li>}
            {search.data.map((c) => (
              <li key={c.external_id} className="flex items-center gap-3 px-3 py-2">
                <Avatar name={c.name} size={24} />
                <Link to={`/bm/${c.external_id}`} className="min-w-0 flex-1 truncate font-medium hover:text-teal">
                  {c.name}
                </Link>
                <span className="text-[11px] text-faint num">
                  BM {c.external_id}
                  {c.last_seen && ` · ${fmtRelative(c.last_seen)}`}
                </span>
                {c.source_url && (
                  <Button size="iconSm" variant="ghost" onClick={() => openExternal(c.source_url!)} aria-label="Открыть на BattleMetrics">
                    <ExternalLink className="size-3.5" />
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
      </Card>
      <div className="grid gap-4 xl:grid-cols-[1.6fr_1fr]">
        <Card>
          <CardHeader
            title="Игроки в базе"
            subtitle="Все, кого программа видела на серверах или открывала"
            actions={
              <>
                <Input value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Фильтр" className="h-7 w-40 text-xs" aria-label="Фильтр игроков" />
                <Segmented label="Показать" value={tracked} onChange={setTracked} options={[{ value: "all", label: "Все" }, { value: "tracked", label: "Отслеживаемые" }]} />
              </>
            }
          />
          {local.isLoading && <LoadingBlock />}
          {local.data?.length === 0 && <EmptyState title="Пока пусто" text="Найдите игрока выше или включите наблюдение за сервером — игроки появятся автоматически." />}
          <ul className="max-h-[560px] divide-y divide-line overflow-y-auto">
            {local.data?.map((p) => (
              <li key={p.bm_id}>
                <Link to={`/bm/${p.bm_id}`} className="flex items-center gap-3 px-4 py-2 hover:bg-raised">
                  <Avatar name={p.name} size={24} demo={p.is_demo} />
                  <span className="min-w-0 flex-1 truncate text-[13px] font-medium">{p.name ?? p.bm_id}</span>
                  {p.tracked && <Badge tone="teal">отслеживается</Badge>}
                  {p.steam_id && <Badge>Steam</Badge>}
                  <span className="w-28 text-right text-[11px] text-faint">{fmtRelative(p.last_seen)}</span>
                </Link>
              </li>
            ))}
          </ul>
        </Card>
        <Card className="h-fit">
          <CardHeader title="Наблюдаемые серверы" icon={<Server className="size-4" />} actions={<Link to="/servers" className="text-xs text-teal hover:underline">Серверы →</Link>} />
          {(watch.data ?? []).filter((w) => w.enabled).length === 0 ? (
            <div className="p-3">
              <Notice tone="info">Включите «Наблюдать» у сервера на странице «Серверы». Программа будет каждые пару минут смотреть, кто онлайн, и строить сессии и тиммейтов.</Notice>
            </div>
          ) : (
            <ul className="divide-y divide-line">
              {watch.data!
                .filter((w) => w.enabled)
                .map((w) => (
                  <li key={w.server_id} className="px-4 py-2 text-[13px]">
                    <div className="truncate">{w.server_name ?? w.server_id}</div>
                    <div className="text-[11px] text-faint">
                      {w.last_error ? <span className="text-amber">ошибка: {w.last_error}</span> : `опрос ${fmtRelative(w.last_success_at)} · онлайн ${w.last_online ?? "—"}`}
                    </div>
                  </li>
                ))}
            </ul>
          )}
        </Card>
      </div>
    </div>
  );
}
