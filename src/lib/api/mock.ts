// Mock-транспорт для запуска UI в обычном браузере (dev, Playwright).
// Данные — фикстуры, сгенерированные Rust-тестом `export_ui_fixtures` из демо-сида: ВЫМЫШЛЕННЫЕ.
import type { AppInfo, AppSettings } from "@/lib/types";
import { parseIdentifierLocal } from "@/lib/validation";
import type { Args } from "./transport";

type Json = Record<string, unknown>;
let data: Json | null = null;
async function fixtures(): Promise<Json> {
  if (!data) data = (await import("./mock-data.json")).default as unknown as Json;
  return data;
}

const defaultSettings: AppSettings = {
  theme: "dark",
  language: "ru",
  timezone: "Europe/Moscow",
  autostart: false,
  close_to_tray: false,
  retention_days: 90,
  poll_interval_secs: 120,
  tracking_paused: false,
  risk_indicators: true,
  reduced_motion: "system",
  density: "compact",
  auto_watch_servers: true,
  notifications: { windows: false, telegram: false, telegram_chat_id: null, quiet_start: null, quiet_end: null, events: {} },
};

const state = {
  mode: (globalThis.localStorage?.getItem("rid-mock-mode") as "demo" | "live" | null) ?? null,
  settings: defaultSettings,
};

export async function mockInvoke(cmd: string, args: Args): Promise<unknown> {
  const f = await fixtures();
  const sid = args.steamId as string | undefined;
  switch (cmd) {
    case "get_app_info": {
      const info: AppInfo = {
        version: "0.1.0-web",
        mode: state.mode,
        first_run: state.mode === null,
        data_dir: "(браузерный mock)",
        settings: state.settings,
        secret_backend: "memory_only",
        schema_version: 1,
        platform: "web",
      };
      return info;
    }
    case "choose_mode":
      if (args.mode === "live") throw { kind: "validation", message: "В браузерном mock доступен только демо-режим. Реальный режим — в настольном приложении." };
      state.mode = "demo";
      globalThis.localStorage?.setItem("rid-mock-mode", "demo");
      return null;
    case "update_settings":
      state.settings = args.settings as AppSettings;
      return state.settings;
    case "parse_identifier":
      return parseIdentifierLocal(String(args.input ?? ""));
    case "search_player": {
      const p = parseIdentifierLocal(String(args.input ?? ""));
      const profiles = f.get_player_profile as Record<string, unknown>;
      if (p.kind === "steam_id") {
        if (!profiles[p.steam_id]) throw { kind: "not_found", message: "В демо-режиме доступны только вымышленные профили." };
        return { parsed: p, resolved_steam_id: p.steam_id, candidates: [], notices: [] };
      }
      if (p.kind === "vanity") throw { kind: "not_found", message: "Vanity URL разрешается через Steam Web API — доступно в реальном режиме." };
      const players = (f.list_players as { steam_id: string; persona_name: string; is_demo: boolean }[]).filter((x) =>
        x.persona_name.toLowerCase().includes(p.text.toLowerCase()),
      );
      return {
        parsed: p,
        resolved_steam_id: null,
        candidates: players.map((x) => ({ steam_id: x.steam_id, name: x.persona_name, provider: "demo", external_id: null, source_url: null, last_seen: null, note: "Из локальной базы", is_demo: true })),
        notices: players.length ? [] : [{ level: "info", text: "Совпадений нет. Вставьте SteamID64 или ссылку на профиль Steam." }],
      };
    }
    case "recent_searches":
      return [];
    case "get_player_profile": {
      const p = (f.get_player_profile as Record<string, unknown>)[sid ?? ""];
      if (!p) throw { kind: "not_found", message: "Игрок не найден в локальной базе" };
      return p;
    }
    case "get_activity": {
      const preset = (args.range as { preset: string }).preset;
      const key = `${sid}:${["7d", "30d", "90d"].includes(preset) ? preset : "7d"}`;
      const r = (f.get_activity as Record<string, unknown>)[key];
      if (!r) throw { kind: "not_found", message: "Нет данных" };
      return r;
    }
    case "get_squad_report": {
      const preset = (args.range as { preset: string }).preset === "7d" ? "7d" : "30d";
      const r = (f.get_squad_report as Record<string, unknown>)[`${args.id}:${preset}`];
      if (!r) throw { kind: "not_found", message: "Отряд не найден" };
      return r;
    }
    case "get_bm_player": {
      const id = String(args.bmId ?? "").replace(/.*\/players\//, "").replace(/\/$/, "");
      const r = (f.get_bm_player as Record<string, unknown>)[id];
      if (!r) throw { kind: "not_found", message: "В демо-режиме доступны только вымышленные игроки BM (откройте их из списка)" };
      return r;
    }
    case "get_bm_activity": {
      const preset = (args.range as { preset: string }).preset;
      const r = (f.get_bm_activity as Record<string, unknown>)[`${args.bmId}:${["7d", "30d", "90d"].includes(preset) ? preset : "7d"}`];
      if (!r) throw { kind: "not_found", message: "Нет данных" };
      return r;
    }
    case "get_server_online": {
      const r = (f.get_server_online as Record<string, unknown>)[String(args.id)];
      if (!r) throw { kind: "not_found", message: "Сервер не найден" };
      return r;
    }
    case "list_bm_players": {
      const q = String(args.query ?? "").toLowerCase();
      const list = f.list_bm_players as { name: string | null; bm_id: string; tracked: boolean }[];
      return list.filter((p) => (!q || (p.name ?? "").toLowerCase().includes(q) || p.bm_id === q) && (!args.trackedOnly || p.tracked));
    }
    case "get_bm_routine":
    case "get_bm_post_wipe":
    case "get_bm_alts": {
      const r = (f[cmd] as Record<string, unknown>)[String(args.bmId)];
      if (r === undefined) throw { kind: "not_found", message: "Нет данных" };
      return r;
    }
    case "list_bm_clans":
      return f.list_bm_clans;
    case "get_bm_clan_report": {
      const preset = (args.range as { preset: string }).preset === "7d" ? "7d" : "30d";
      const r = (f.get_bm_clan_report as Record<string, unknown>)[`${args.id}:${preset}`];
      if (!r) throw { kind: "not_found", message: "Клан не найден" };
      return r;
    }
    case "get_server_insights": {
      const r = (f.get_server_insights as Record<string, unknown>)[String(args.id)];
      if (!r) throw { kind: "not_found", message: "Нет данных" };
      return r;
    }
    case "list_server_watch":
      return f.list_server_watch;
    case "get_server_population": {
      const r = (f.get_server_population as Record<string, unknown>)[String(args.id)];
      if (!r) throw { kind: "not_found", message: "Сервер не найден" };
      return r;
    }
    case "list_players":
    case "list_squads":
    case "list_tracked":
    case "list_events":
    case "get_bans_overview":
    case "list_servers":
    case "get_integrations":
    case "get_dashboard":
      return f[cmd];
    case "get_tracker_status":
      return (f.get_dashboard as { tracker: unknown }).tracker;
    case "refresh_servers":
      return { fetched: 0, errors: ["Демо-режим: каталог вымышленный и не обновляется из сети"], sources: ["demo"] };
    case "get_diagnostics":
      return {
        integrity: "ok",
        schema_version: 1,
        db_path: "(браузерный mock)",
        db_size_bytes: 0,
        counts: [],
        health: [],
        tracker: (f.get_dashboard as { tracker: unknown }).tracker,
        secret_backend: "memory_only",
        generated_at: new Date().toISOString(),
      };
    case "poll_now":
      return 0;
    case "get_bm_link_candidates":
    case "search_battlemetrics":
    case "link_battlemetrics":
      throw { kind: "validation", message: "В демо-режиме BattleMetrics не используется" };
    case "test_integration":
    case "set_secret":
      throw { kind: "validation", message: "Интеграции доступны только в настольном приложении" };
    default:
      // Мутации в mock не сохраняются.
      return null;
  }
}
