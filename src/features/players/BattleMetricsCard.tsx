import { useState } from "react";
import { Link } from "react-router";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ExternalLink, Link2, Link2Off, RefreshCw, Search } from "lucide-react";
import { api } from "@/lib/api";
import type { PlayerProfile } from "@/lib/types";
import { fmtRelative } from "@/lib/format";
import { openExternal } from "@/lib/external";
import { useAppInfo } from "@/app/hooks";
import { Badge, Button, Card, CardHeader, Dialog, EmptyState, Input, Notice } from "@/components/ui";

/** Привязка игрока BattleMetrics.com к профилю Steam: автоматически по SteamID или вручную (ник / ссылка). */
export function BattleMetricsCard({ p }: { p: PlayerProfile }) {
  const qc = useQueryClient();
  const info = useAppInfo();
  const [open, setOpen] = useState(false);
  const link = p.links.find((l) => l.provider === "battlemetrics");
  const sid = p.player.steam_id;
  const inv = () => {
    qc.invalidateQueries({ queryKey: ["profile", sid] });
    qc.invalidateQueries({ queryKey: ["activity", sid] });
    qc.invalidateQueries({ queryKey: ["dashboard"] });
  };
  const showNotices = (n: { level: string; text: string }[]) =>
    n.forEach((x) => (x.level === "error" ? toast.error(x.text) : x.level === "warning" ? toast.warning(x.text) : toast.success(x.text)));
  const refresh = useMutation({ mutationFn: () => api.refreshPlayer(sid), onSuccess: (n) => (inv(), showNotices(n)), onError: (e) => toast.error(e.message) });
  const unlink = useMutation({ mutationFn: () => api.unlinkBattlemetrics(sid), onSuccess: () => (inv(), toast.success("Привязка удалена")) });

  if (info.data?.mode === "demo" || p.player.is_demo) return null;

  return (
    <Card>
      <CardHeader
        title="BattleMetrics"
        subtitle="Серверы, время на них, онлайн и сессии"
        actions={
          link ? (
            <Badge tone={link.match_evidence === "exact_identifier" ? "teal" : "amber"} title={link.match_evidence === "exact_identifier" ? "Совпадение по SteamID" : "Привязано вами вручную"}>
              {link.match_evidence === "exact_identifier" ? "по SteamID" : "вручную"}
            </Badge>
          ) : (
            <Badge>не привязан</Badge>
          )
        }
      />
      <div className="space-y-2 p-3 text-[13px]">
        {link ? (
          <>
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-muted">Игрок BM:</span>
              <button type="button" className="flex items-center gap-1 text-teal hover:underline num" onClick={() => openExternal(`https://www.battlemetrics.com/players/${link.external_id}`)}>
                {link.external_id} <ExternalLink className="size-3" />
              </button>
              <span className="text-[11px] text-faint">привязан {fmtRelative(link.linked_at)}</span>
              <Link to={`/bm/${link.external_id}`} className="text-xs text-teal hover:underline">
                Страница игрока BM → тиммейты, сессии
              </Link>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button size="sm" onClick={() => refresh.mutate()} loading={refresh.isPending}>
                <RefreshCw className="size-3.5" /> Загрузить данные
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setOpen(true)}>
                <Link2 className="size-3.5" /> Сменить
              </Button>
              <Button size="sm" variant="ghost" onClick={() => unlink.mutate()} loading={unlink.isPending}>
                <Link2Off className="size-3.5" /> Отвязать
              </Button>
            </div>
          </>
        ) : (
          <>
            <p className="text-xs text-muted">
              BattleMetrics не раскрывает SteamID обычным токенам, поэтому игрока часто нельзя найти автоматически. Найдите его по нику или вставьте ссылку на его страницу BattleMetrics.
            </p>
            <Button size="sm" variant="primary" onClick={() => setOpen(true)}>
              <Link2 className="size-3.5" /> Привязать BattleMetrics
            </Button>
          </>
        )}
      </div>
      <LinkDialog open={open} onClose={() => setOpen(false)} p={p} onLinked={(n) => (inv(), showNotices(n), setOpen(false))} />
    </Card>
  );
}

function LinkDialog({ open, onClose, p, onLinked }: { open: boolean; onClose: () => void; p: PlayerProfile; onLinked: (n: { level: string; text: string }[]) => void }) {
  const [name, setName] = useState(p.player.persona_name ?? "");
  const [manual, setManual] = useState("");
  const search = useMutation({ mutationFn: () => api.searchBattlemetrics(name), onError: (e) => toast.error(e.message) });
  const link = useMutation({ mutationFn: (input: string) => api.linkBattlemetrics(p.player.steam_id, input), onSuccess: onLinked, onError: (e) => toast.error(e.message) });
  const knownNames = new Set(p.aliases.map((a) => a.name.toLowerCase()));
  return (
    <Dialog open={open} onClose={onClose} title="Привязать игрока BattleMetrics" wide>
      <div className="space-y-4 text-[13px]">
        <div className="space-y-2">
          <div className="font-medium">1. Найти по нику</div>
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              search.mutate();
            }}
          >
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Ник игрока" aria-label="Ник для поиска в BattleMetrics" maxLength={64} />
            <Button type="submit" loading={search.isPending} disabled={name.trim().length < 2}>
              <Search className="size-3.5" /> Найти
            </Button>
          </form>
          {search.data && search.data.length === 0 && <EmptyState title="Никого не найдено" text="Попробуйте другой ник из истории ников или вставьте ссылку вручную." />}
          {search.data && search.data.length > 0 && (
            <ul className="max-h-64 divide-y divide-line overflow-y-auto rounded-md border border-line">
              {search.data.map((c) => (
                <li key={c.external_id} className="flex items-center gap-3 px-3 py-2">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="truncate font-medium">{c.name}</span>
                      {knownNames.has(c.name.toLowerCase()) && <Badge tone="teal">совпадает с ником Steam</Badge>}
                    </div>
                    <div className="text-[11px] text-faint num">
                      BM {c.external_id}
                      {c.last_seen && ` · активность ${fmtRelative(c.last_seen)}`}
                    </div>
                  </div>
                  {c.source_url && (
                    <Button size="iconSm" variant="ghost" onClick={() => openExternal(c.source_url!)} aria-label="Открыть на BattleMetrics" title="Проверить на сайте">
                      <ExternalLink className="size-3.5" />
                    </Button>
                  )}
                  <Button size="sm" onClick={() => link.mutate(c.external_id!)} loading={link.isPending && link.variables === c.external_id}>
                    Это он
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="space-y-2">
          <div className="font-medium">2. Или вставить ссылку / ID</div>
          <div className="flex gap-2">
            <Input value={manual} onChange={(e) => setManual(e.target.value)} placeholder="https://www.battlemetrics.com/players/123456789" aria-label="Ссылка или ID игрока BattleMetrics" maxLength={200} />
            <Button variant="primary" onClick={() => link.mutate(manual)} loading={link.isPending && link.variables === manual} disabled={!manual.trim()}>
              Привязать
            </Button>
          </div>
          <p className="text-[11px] text-faint">На battlemetrics.com найдите игрока, откройте его страницу и скопируйте адрес из браузера.</p>
        </div>
        <Notice tone="warning">Ники не уникальны. Перед привязкой откройте кандидата на сайте и сверьте серверы и время активности. Привязка помечается как «вручную».</Notice>
      </div>
    </Dialog>
  );
}
