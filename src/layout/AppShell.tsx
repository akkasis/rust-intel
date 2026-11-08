import { NavLink, Outlet, useNavigate } from "react-router";
import { useEffect } from "react";
import {
  Activity,
  Ban,
  ChevronsLeft,
  ChevronsRight,
  Gauge,
  Plug,
  Radar,
  Search,
  Server,
  Settings,
  Users,
  UserRound,
} from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { listen } from "@/lib/api/transport";
import { useAppInfo, useTz } from "@/app/hooks";
import { useUi } from "@/app/store";
import { Badge, Switch, cn } from "@/components/ui";
import { CommandPalette } from "./CommandPalette";
import { Logo } from "./Logo";

const NAV = [
  { to: "/", label: "Обзор", icon: Gauge, end: true },
  { to: "/search", label: "Поиск игрока", icon: Search },
  { to: "/players", label: "Игроки", icon: UserRound },
  { to: "/activity", label: "Активность", icon: Activity },
  { to: "/squads", label: "Отряды", icon: Users },
  { to: "/tracker", label: "Live-трекер", icon: Radar },
  { to: "/bans", label: "Баны и риски", icon: Ban },
  { to: "/servers", label: "Серверы", icon: Server },
  { to: "/integrations", label: "Интеграции", icon: Plug },
  { to: "/settings", label: "Настройки", icon: Settings },
];

export function AppShell() {
  const info = useAppInfo();
  const qc = useQueryClient();
  const nav = useNavigate();
  const { sidebarCollapsed, toggleSidebar, setPalette, utc, setUtc } = useUi();
  const tz = useTz();
  const unread = useQuery({ queryKey: ["dashboard"], queryFn: api.dashboard, refetchInterval: 30_000 });
  const demo = info.data?.mode === "demo";

  useEffect(() => {
    const unsubs: Promise<() => void>[] = [
      listen("rid://events", () => {
        qc.invalidateQueries({ queryKey: ["dashboard"] });
        qc.invalidateQueries({ queryKey: ["events"] });
        qc.invalidateQueries({ queryKey: ["tracked"] });
      }),
      listen("rid://settings", () => qc.invalidateQueries({ queryKey: ["app-info"] })),
    ];
    return () => unsubs.forEach((p) => p.then((u) => u()));
  }, [qc]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette(true);
      }
      if (e.altKey && /^[1-9]$/.test(e.key)) {
        const item = NAV[Number(e.key) - 1];
        if (item) nav(item.to);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [nav, setPalette]);

  return (
    <div className="flex h-full flex-col">
      {demo && (
        <div className="flex h-6 shrink-0 items-center justify-center gap-2 bg-violet/15 text-[11px] font-medium tracking-wide text-violet" role="status">
          <span className="rounded bg-violet/25 px-1.5">ДЕМО</span>
          Все игроки, серверы и события вымышлены. Реальные данные — в режиме «Live» (Настройки).
        </div>
      )}
      <div className="flex min-h-0 flex-1">
        <aside className={cn("flex shrink-0 flex-col border-r border-line bg-panel transition-[width]", sidebarCollapsed ? "w-[52px]" : "w-[208px]")} aria-label="Навигация">
          <div className={cn("flex h-12 items-center gap-2 border-b border-line", sidebarCollapsed ? "justify-center" : "px-3")}>
            <Logo size={24} />
            {!sidebarCollapsed && (
              <div className="leading-tight">
                <div className="font-display text-[13px] font-bold tracking-wide">RID</div>
                <div className="text-[10px] text-faint">Rust Intelligence</div>
              </div>
            )}
          </div>
          <nav className="flex-1 space-y-0.5 overflow-y-auto p-2">
            {NAV.map((n, i) => (
              <NavLink
                key={n.to}
                to={n.to}
                end={n.end}
                title={`${n.label} (Alt+${i + 1})`}
                className={({ isActive }) =>
                  cn(
                    "group flex h-8 items-center gap-2.5 rounded-md text-[13px] font-medium transition-colors",
                    sidebarCollapsed ? "justify-center" : "px-2.5",
                    isActive ? "bg-teal/10 text-fg shadow-[inset_2px_0_0_var(--rid-teal)]" : "text-muted hover:bg-raised hover:text-fg",
                  )
                }
              >
                <n.icon className="size-4 shrink-0" aria-hidden />
                {!sidebarCollapsed && <span className="truncate">{n.label}</span>}
                {!sidebarCollapsed && n.to === "/tracker" && (unread.data?.unread_events ?? 0) > 0 && (
                  <span className="ml-auto rounded bg-teal/15 px-1.5 text-[10px] text-teal num">{unread.data?.unread_events}</span>
                )}
              </NavLink>
            ))}
          </nav>
          <button
            type="button"
            onClick={toggleSidebar}
            className="flex h-9 items-center justify-center border-t border-line text-faint hover:text-fg"
            aria-label={sidebarCollapsed ? "Развернуть меню" : "Свернуть меню"}
          >
            {sidebarCollapsed ? <ChevronsRight className="size-4" /> : <ChevronsLeft className="size-4" />}
          </button>
        </aside>
        <div className="flex min-w-0 flex-1 flex-col">
          <header className="flex h-12 shrink-0 items-center gap-3 border-b border-line bg-panel/60 px-4">
            <button
              type="button"
              onClick={() => setPalette(true)}
              className="flex h-8 w-full max-w-[460px] items-center gap-2 rounded-md border border-line bg-panel-2 px-2.5 text-left text-[13px] text-faint hover:border-line-strong"
              aria-label="Открыть командный поиск"
            >
              <Search className="size-3.5" />
              <span className="flex-1 truncate">SteamID64, ссылка на профиль, STEAM_0:…, [U:1:…] или команда</span>
              <kbd className="rounded border border-line bg-raised px-1 font-mono text-[10px] text-muted">Ctrl K</kbd>
            </button>
            <div className="ml-auto flex items-center gap-3 text-xs text-muted">
              <label className="flex items-center gap-1.5" title="Переключить отображение времени: UTC или пояс из настроек">
                <Switch checked={utc} onChange={setUtc} label="Показывать время в UTC" />
                <span className="num">{utc ? "UTC" : tz}</span>
              </label>
              {info.data && (
                <Badge tone={demo ? "violet" : "teal"} title={demo ? "Демо-режим: вымышленные данные" : "Реальный режим: только подтверждённые источники"}>
                  {demo ? "Демо" : "Live"}
                </Badge>
              )}
            </div>
          </header>
          <main className="min-h-0 flex-1 overflow-y-auto">
            <div className="mx-auto w-full max-w-[1600px] px-5 py-4">
              <Outlet />
            </div>
          </main>
        </div>
      </div>
      <CommandPalette />
    </div>
  );
}
