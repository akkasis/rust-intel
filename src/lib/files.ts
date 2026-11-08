import { isTauri } from "./api/transport";

export async function pickSavePath(defaultPath: string, ext: string, label: string): Promise<string | null> {
  if (!isTauri()) throw new Error("Работа с файлами доступна только в настольном приложении");
  const { save } = await import("@tauri-apps/plugin-dialog");
  return save({ defaultPath, filters: [{ name: label, extensions: [ext] }] });
}

export async function pickOpenPath(ext: string[], label: string): Promise<string | null> {
  if (!isTauri()) throw new Error("Работа с файлами доступна только в настольном приложении");
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ multiple: false, directory: false, filters: [{ name: label, extensions: ext }] });
  return typeof r === "string" ? r : null;
}
