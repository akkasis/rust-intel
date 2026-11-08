import { isTauri } from "./api/transport";

const ALLOWED = ["steamcommunity.com", "www.battlemetrics.com", "battlemetrics.ru", "partner.steamgames.com", "core.telegram.org"];

export function isAllowedUrl(u: string): boolean {
  try {
    const url = new URL(u);
    return url.protocol === "https:" && ALLOWED.includes(url.hostname);
  } catch {
    return false;
  }
}

/** Открывает только https-ссылки на известные источники. */
export async function openExternal(u: string): Promise<void> {
  if (!isAllowedUrl(u)) return;
  if (isTauri()) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(u);
  } else {
    window.open(u, "_blank", "noopener,noreferrer");
  }
}
