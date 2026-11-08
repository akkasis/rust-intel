import { describe, expect, it } from "vitest";
import { fmtApprox, fmtDuration, fmtPct, fmtRelative, plural } from "./format";
import { parseIdentifierLocal } from "./validation";

describe("format", () => {
  it("склоняет числительные", () => {
    expect(plural(1, "день", "дня", "дней")).toBe("день");
    expect(plural(3, "день", "дня", "дней")).toBe("дня");
    expect(plural(11, "день", "дня", "дней")).toBe("дней");
    expect(plural(21, "день", "дня", "дней")).toBe("день");
  });
  it("форматирует длительности и проценты", () => {
    expect(fmtDuration(0)).toBe("0 мин");
    expect(fmtDuration(5400)).toBe("1 ч 30 мин");
    expect(fmtPct(null)).toBe("—");
    expect(fmtPct(0.456)).toBe("46%");
  });
  it("показывает интервал неопределённости в часовом поясе", () => {
    const s = fmtApprox("2026-11-01T10:00:00Z", "2026-11-01T10:05:00Z", "Europe/Moscow");
    expect(s).toContain("13:00");
    expect(s).toContain("13:05");
    expect(s.startsWith("≈")).toBe(true);
  });
  it("относительное время", () => {
    const now = new Date("2026-11-01T12:00:00Z");
    expect(fmtRelative("2026-11-01T11:00:00Z", now)).toBe("1 час назад");
    expect(fmtRelative(null, now)).toBe("никогда");
  });
});

describe("локальная валидация идентификаторов (зеркало Rust)", () => {
  it("SteamID64, Steam2, Steam3, URL", () => {
    expect(parseIdentifierLocal("76561197960287930")).toMatchObject({ kind: "steam_id", steam_id: "76561197960287930" });
    expect(parseIdentifierLocal("STEAM_0:0:11101")).toMatchObject({ steam_id: "76561197960287930" });
    expect(parseIdentifierLocal("[U:1:22202]")).toMatchObject({ steam_id: "76561197960287930" });
    expect(parseIdentifierLocal("https://steamcommunity.com/profiles/76561197960287930/")).toMatchObject({ steam_id: "76561197960287930" });
    expect(parseIdentifierLocal("https://steamcommunity.com/id/some_name/")).toMatchObject({ kind: "vanity", vanity: "some_name" });
    expect(parseIdentifierLocal("Ворон")).toMatchObject({ kind: "ambiguous" });
    expect(() => parseIdentifierLocal("123")).toThrow();
  });
  it("не теряет точность SteamID64 (string, не number)", () => {
    const r = parseIdentifierLocal("STEAM_0:1:2050000000");
    expect(r).toMatchObject({ steam_id: "76561202060265729" });
  });
});
