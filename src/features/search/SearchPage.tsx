import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ClipboardPaste, ExternalLink, Search } from "lucide-react";
import { api } from "@/lib/api";
import { parseIdentifierLocal } from "@/lib/validation";
import type { SearchOutcome } from "@/lib/types";
import { PROVIDER_NAMES, fmtRelative } from "@/lib/format";
import { openExternal } from "@/lib/external";
import { Badge, Button, Card, CardHeader, Input, Notice, PageHeader } from "@/components/ui";
import { useIsDemo } from "@/app/hooks";

const FORMATS = [
  ["SteamID64", "76561197960287930"],
  ["Ссылка на профиль", "https://steamcommunity.com/profiles/7656…"],
  ["Vanity URL", "https://steamcommunity.com/id/name"],
  ["Steam2", "STEAM_0:0:11101"],
  ["Steam3", "[U:1:22202]"],
] as const;

export function SearchPage() {
  const [params, setParams] = useSearchParams();
  const [q, setQ] = useState(params.get("q") ?? "");
  const [result, setResult] = useState<SearchOutcome | null>(null);
  const nav = useNavigate();
  const qc = useQueryClient();
  const demo = useIsDemo();
  const recent = useQuery({ queryKey: ["recent-searches"], queryFn: api.recentSearches });
  const players = useQuery({ queryKey: ["players", ""], queryFn: () => api.players(), enabled: demo });

  const preview = useMemo(() => {
    if (!q.trim()) return null;
    try {
      const p = parseIdentifierLocal(q);
      if (p.kind === "steam_id") return { ok: true, text: `SteamID64 ${p.steam_id} (из формата ${p.from_format})` };
      if (p.kind === "vanity") return { ok: true, text: `Vanity URL «${p.vanity}» — будет разрешён через Steam Web API` };
      return { ok: true, text: "Свободный текст: поиск по нику только в локальной базе и у источников с поддержкой поиска" };
    } catch (e) {
      return { ok: false, text: (e as { message?: string }).message ?? "Некорректный ввод" };
    }
  }, [q]);

  const m = useMutation({
    mutationFn: (input: string) => api.search(input),
    onSuccess: (r) => {
      setResult(r);
      qc.invalidateQueries({ queryKey: ["recent-searches"] });
      qc.invalidateQueries({ queryKey: ["players"] });
      if (r.resolved_steam_id && r.notices.every((n) => n.level !== "error")) nav(`/players/${r.resolved_steam_id}`, { state: { notices: r.notices } });
    },
  });

  useEffect(() => {
    const initial = params.get("q");
    if (initial) m.mutate(initial);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const submit = (v = q) => {
    if (!v.trim()) return;
    const bm = /battlemetrics\.com\/(?:rcon\/)?players\/(\d{1,15})/i.exec(v);
    if (bm) {
      nav(`/bm/${bm[1]}`);
      return;
    }
    setParams({ q: v.trim() }, { replace: true });
    m.mutate(v.trim());
  };

  const paste = async () => {
    try {
      const t = await navigator.clipboard.readText();
      setQ(t.trim());
      submit(t.trim());
    } catch {
      /* буфер обмена недоступен */
    }
  };

  return (
    <div className="space-y-4">
      <PageHeader title="Поиск игрока" subtitle="Идентификатор проверяется локально до запроса к источникам. Совпадения по нику никогда не выбираются автоматически." />
      <Card className="p-4">
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute top-2 left-2.5 size-4 text-faint" />
            <Input
              autoFocus
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Вставьте SteamID64, ссылку на профиль Steam, STEAM_0:X:Y или [U:1:N]"
              className="h-9 pl-8 text-sm"
              aria-label="Идентификатор игрока"
              aria-invalid={preview ? !preview.ok : undefined}
              aria-describedby="search-preview"
              maxLength={300}
            />
          </div>
          <Button type="button" onClick={paste} title="Вставить из буфера обмена" aria-label="Вставить из буфера обмена" size="icon" className="h-9 w-9">
            <ClipboardPaste className="size-4" />
          </Button>
          <Button type="submit" variant="primary" className="h-9 px-4" loading={m.isPending} disabled={preview?.ok === false}>
            Найти
          </Button>
        </form>
        <div id="search-preview" className={`mt-2 min-h-4 text-xs ${preview?.ok === false ? "text-danger" : "text-muted"}`} aria-live="polite">
          {preview?.text}
        </div>
        <div className="mt-3 flex flex-wrap gap-1.5">
          {FORMATS.map(([l, ex]) => (
            <span key={l} className="rounded border border-line bg-panel-2 px-2 py-0.5 text-[11px] text-muted" title={ex}>
              {l}
            </span>
          ))}
        </div>
      </Card>

      {m.error && <Notice tone="error">{m.error.message}</Notice>}
      {result && (
        <Card>
          <CardHeader title="Результаты" subtitle={result.resolved_steam_id ? `Найден SteamID64 ${result.resolved_steam_id}` : `Кандидатов: ${result.candidates.length}`} />
          <div className="space-y-2 p-3">
            {result.notices.map((n, i) => (
              <Notice key={i} tone={n.level === "error" ? "error" : n.level === "warning" ? "warning" : "info"}>
                {n.text}
              </Notice>
            ))}
            {result.candidates.length > 0 && (
              <ul className="divide-y divide-line rounded-md border border-line">
                {result.candidates.map((c, i) => (
                  <li key={i} className="flex items-center gap-3 px-3 py-2">
                    <div className="min-w-0 flex-1">
                      <div className="font-medium">{c.name}</div>
                      <div className="text-[11px] text-faint">
                        {PROVIDER_NAMES[c.provider] ?? c.provider} · {c.note}
                        {c.last_seen && ` · ${fmtRelative(c.last_seen)}`}
                      </div>
                    </div>
                    {c.steam_id ? (
                      <Link to={`/players/${c.steam_id}`}>
                        <Button size="sm">Открыть</Button>
                      </Link>
                    ) : (
                      <Badge tone="amber" title="SteamID не подтверждён — откройте источник и скопируйте SteamID вручную">
                        SteamID не подтверждён
                      </Badge>
                    )}
                    {c.source_url && (
                      <Button size="iconSm" variant="ghost" onClick={() => openExternal(c.source_url!)} aria-label="Открыть источник">
                        <ExternalLink className="size-3.5" />
                      </Button>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </div>
        </Card>
      )}

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader title="Недавние запросы" />
          <ul className="divide-y divide-line">
            {(recent.data ?? []).length === 0 && <li className="px-4 py-3 text-xs text-muted">История пуста</li>}
            {(recent.data ?? []).map((r) => (
              <li key={r.query}>
                <button type="button" className="flex w-full items-center justify-between gap-2 px-4 py-2 text-left hover:bg-raised" onClick={() => (setQ(r.query), submit(r.query))}>
                  <span className="truncate text-[13px] num">{r.query}</span>
                  <span className="text-[11px] text-faint">{fmtRelative(r.searched_at)}</span>
                </button>
              </li>
            ))}
          </ul>
        </Card>
        {demo && (
          <Card>
            <CardHeader title="Вымышленные профили демо-режима" subtitle="Нажмите, чтобы подставить SteamID" />
            <ul className="divide-y divide-line">
              {(players.data ?? []).map((p) => (
                <li key={p.steam_id}>
                  <button type="button" className="flex w-full items-center justify-between gap-2 px-4 py-2 text-left hover:bg-raised" onClick={() => (setQ(p.steam_id), submit(p.steam_id))}>
                    <span className="text-[13px]">{p.persona_name}</span>
                    <span className="text-[11px] text-faint num">{p.steam_id}</span>
                  </button>
                </li>
              ))}
            </ul>
          </Card>
        )}
      </div>
    </div>
  );
}
