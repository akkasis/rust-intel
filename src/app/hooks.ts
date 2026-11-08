import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { useUi } from "./store";

export function useAppInfo() {
  return useQuery({ queryKey: ["app-info"], queryFn: api.appInfo, staleTime: 30_000 });
}

/** Часовой пояс отображения: из настроек или UTC (переключатель в шапке). */
export function useTz(): string {
  const utc = useUi((s) => s.utc);
  const info = useAppInfo();
  if (utc) return "UTC";
  return info.data?.settings.timezone ?? "UTC";
}

export function useIsDemo(): boolean {
  return useAppInfo().data?.mode === "demo";
}
