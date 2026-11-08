import type { AppErrorPayload } from "@/lib/types";

export type Args = Record<string, unknown>;

export class ApiError extends Error {
  payload: AppErrorPayload;
  constructor(payload: AppErrorPayload) {
    super(payload.message);
    this.payload = payload;
  }
}

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function toApiError(e: unknown): ApiError {
  if (e instanceof ApiError) return e;
  if (e && typeof e === "object" && "kind" in e && "message" in e) return new ApiError(e as AppErrorPayload);
  return new ApiError({ kind: "internal", message: typeof e === "string" ? e : "Неизвестная ошибка" });
}

/** Вызов команды Rust-ядра. В браузере (dev/e2e) — mock на фикстурах демо-сида. */
export async function call<T>(cmd: string, args: Args = {}): Promise<T> {
  try {
    if (isTauri()) {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<T>(cmd, args);
    }
    if (import.meta.env.TAURI_ENV_PLATFORM) throw new ApiError({ kind: "internal", message: "Нет связи с ядром приложения" });
    const { mockInvoke } = await import("./mock");
    return (await mockInvoke(cmd, args)) as T;
  } catch (e) {
    throw toApiError(e);
  }
}

export async function listen(event: string, cb: () => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen(event, cb);
}
