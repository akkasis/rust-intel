import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { CornerDownLeft, Search, UserRound } from "lucide-react";
import { api } from "@/lib/api";
import { useUi } from "@/app/store";
import { Avatar, cn } from "@/components/ui";
import { parseIdentifierLocal } from "@/lib/validation";

const COMMANDS = [
  { label: "Обзор", to: "/" },
  { label: "Поиск игрока", to: "/search" },
  { label: "Отряды", to: "/squads" },
  { label: "Live-трекер", to: "/tracker" },
  { label: "Баны и риски", to: "/bans" },
  { label: "Серверы", to: "/servers" },
  { label: "Интеграции", to: "/integrations" },
  { label: "Настройки", to: "/settings" },
];

function safeParse(t: string): ReturnType<typeof parseIdentifierLocal> | null {
  try {
    return parseIdentifierLocal(t);
  } catch {
    return null;
  }
}

export function CommandPalette() {
  const { paletteOpen, setPalette } = useUi();
  const [q, setQ] = useState("");
  const [idx, setIdx] = useState(0);
  const nav = useNavigate();
  const input = useRef<HTMLInputElement>(null);
  const players = useQuery({ queryKey: ["players", ""], queryFn: () => api.players(), enabled: paletteOpen });

  useEffect(() => {
    if (paletteOpen) {
      setQ("");
      setIdx(0);
      setTimeout(() => input.current?.focus(), 0);
    }
  }, [paletteOpen]);

  const items = useMemo(() => {
    const out: { key: string; label: string; sub?: string; run: () => void; player?: { name: string | null; demo: boolean } }[] = [];
    const t = q.trim();
    if (t) {
      const parsed = safeParse(t);
      if (parsed && parsed.kind !== "ambiguous") {
        out.push({ key: "search", label: `Найти: ${t}`, sub: parsed.kind === "steam_id" ? `SteamID64 ${parsed.steam_id}` : `vanity «${parsed.vanity}»`, run: () => nav(`/search?q=${encodeURIComponent(t)}`) });
      }
    }
    const lq = t.toLowerCase();
    for (const p of players.data ?? []) {
      if (!lq || p.persona_name?.toLowerCase().includes(lq) || p.steam_id.includes(lq)) {
        out.push({ key: p.steam_id, label: p.persona_name ?? p.steam_id, sub: p.steam_id, run: () => nav(`/players/${p.steam_id}`), player: { name: p.persona_name, demo: p.is_demo } });
      }
      if (out.length > 8) break;
    }
    for (const c of COMMANDS) if (!lq || c.label.toLowerCase().includes(lq)) out.push({ key: c.to, label: c.label, sub: "Перейти", run: () => nav(c.to) });
    if (t && !out.some((o) => o.key === "search")) {
      const item = { key: "search", label: `Искать «${t}»`, sub: "Поиск по нику доступен только при поддержке источника", run: () => nav(`/search?q=${encodeURIComponent(t)}`) };
      // Найденные в базе игроки важнее общего поиска.
      if (out.some((o) => o.player)) out.push(item);
      else out.unshift(item);
    }
    return out.slice(0, 14);
  }, [q, players.data, nav]);

  if (!paletteOpen) return null;
  const run = (i: number) => {
    items[i]?.run();
    setPalette(false);
  };
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/55 pt-[12vh]" onClick={() => setPalette(false)}>
      <div
        role="dialog"
        aria-label="Командный поиск"
        className="w-[min(620px,92vw)] overflow-hidden rounded-xl border border-line-strong bg-panel shadow-2xl"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") setPalette(false);
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setIdx((i) => Math.min(i + 1, items.length - 1));
          }
          if (e.key === "ArrowUp") {
            e.preventDefault();
            setIdx((i) => Math.max(i - 1, 0));
          }
          if (e.key === "Enter") run(idx);
        }}
      >
        <div className="flex items-center gap-2 border-b border-line px-3">
          <Search className="size-4 text-muted" />
          <input
            ref={input}
            value={q}
            onChange={(e) => {
              setQ(e.target.value);
              setIdx(0);
            }}
            placeholder="SteamID64, ссылка, ник из базы или раздел…"
            className="h-12 flex-1 bg-transparent text-sm text-fg outline-none placeholder:text-faint"
            aria-label="Запрос"
            role="combobox"
            aria-expanded
            aria-controls="rid-palette-list"
          />
        </div>
        <ul id="rid-palette-list" role="listbox" className="max-h-[50vh] overflow-y-auto p-1.5">
          {items.map((it, i) => (
            <li
              key={it.key + i}
              role="option"
              aria-selected={i === idx}
              onMouseEnter={() => setIdx(i)}
              onClick={() => run(i)}
              className={cn("flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-1.5", i === idx ? "bg-raised" : "")}
            >
              {it.player ? <Avatar name={it.player.name} size={22} demo={it.player.demo} /> : <UserRound className="size-4 text-faint" />}
              <div className="min-w-0 flex-1">
                <div className="truncate text-[13px]">{it.label}</div>
                {it.sub && <div className="truncate text-[11px] text-faint num">{it.sub}</div>}
              </div>
              {i === idx && <CornerDownLeft className="size-3.5 text-faint" />}
            </li>
          ))}
          {items.length === 0 && <li className="px-3 py-6 text-center text-xs text-muted">Ничего не найдено</li>}
        </ul>
      </div>
    </div>
  );
}
