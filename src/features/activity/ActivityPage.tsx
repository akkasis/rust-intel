import { useSearchParams } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Card, EmptyState, PageHeader, Select } from "@/components/ui";
import { ActivityView } from "./ActivityView";

export function ActivityPage() {
  const [params, setParams] = useSearchParams();
  const players = useQuery({ queryKey: ["players", ""], queryFn: () => api.players() });
  const sel = params.get("player") ?? players.data?.[0]?.steam_id ?? "";
  return (
    <div className="space-y-4">
      <PageHeader
        title="Активность"
        subtitle="Сессии, тепловая карта 7×24, дни недели. Знаменатель — только наблюдаемое время."
        actions={
          <Select value={sel} onChange={(e) => setParams({ player: e.target.value })} aria-label="Игрок" className="min-w-[240px]">
            {(players.data ?? []).map((p) => (
              <option key={p.steam_id} value={p.steam_id}>
                {p.persona_name ?? p.steam_id}
              </option>
            ))}
          </Select>
        }
      />
      {sel ? (
        <ActivityView steamId={sel} />
      ) : (
        <Card>
          <EmptyState title="Нет игроков" text="Найдите игрока через поиск — затем здесь появится его активность." />
        </Card>
      )}
    </div>
  );
}
