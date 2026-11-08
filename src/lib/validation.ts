// Локальная валидация идентификаторов (зеркало Rust domain::ids) — мгновенная подсказка до запроса.
// BigInt: SteamID64 не помещается в Number без потери точности.
import type { ParsedInput } from "@/lib/types";

const BASE = 76561197960265728n;

export function parseIdentifierLocal(raw: string): ParsedInput {
  const s = raw.trim();
  if (!s) throw { kind: "validation", message: "Пустой ввод" };
  if (/^\d+$/.test(s)) {
    if (s.length !== 17) throw { kind: "validation", message: "Это не SteamID64 (ожидается 17 цифр, начинается с 7656119)" };
    const v = BigInt(s);
    if (v <= BASE || v - BASE > 4294967295n) throw { kind: "validation", message: "SteamID вне диапазона индивидуальных аккаунтов" };
    return { kind: "steam_id", steam_id: s, from_format: "steamid64" };
  }
  const m2 = /^STEAM_([01]):([01]):(\d+)$/i.exec(s);
  if (m2) return { kind: "steam_id", steam_id: (BASE + BigInt(m2[3]!) * 2n + BigInt(m2[2]!)).toString(), from_format: "steam2" };
  const m3 = /^\[?U:1:(\d+)\]?$/i.exec(s);
  if (m3) return { kind: "steam_id", steam_id: (BASE + BigInt(m3[1]!)).toString(), from_format: "steam3" };
  const mp = /steamcommunity\.com\/profiles\/(\d{17})/i.exec(s);
  if (mp) return { kind: "steam_id", steam_id: mp[1]!, from_format: "profile_url" };
  const mv = /(?:steamcommunity\.com\/)?id\/([A-Za-z0-9_-]{2,32})\/?$/i.exec(s);
  if (mv) return { kind: "vanity", vanity: mv[1]! };
  return { kind: "ambiguous", text: s.slice(0, 64) };
}
