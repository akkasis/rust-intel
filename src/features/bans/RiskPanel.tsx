import type { RiskSummary } from "@/lib/types";
import { Badge, Card, CardHeader, Notice, cn } from "@/components/ui";
import { Link } from "react-router";

export function RiskPanel({ risk }: { risk: RiskSummary }) {
  return (
    <Card>
      <CardHeader
        title="Индикаторы риска"
        subtitle="Прозрачная сумма публичных сигналов"
        actions={risk.enabled ? <Badge tone={risk.score >= 3 ? "amber" : "neutral"}>{`${(risk.score + 0).toFixed(2)} / ${risk.max_score.toFixed(2)}`}</Badge> : <Badge>выключено</Badge>}
      />
      {!risk.enabled ? (
        <div className="p-3 text-xs text-muted">
          Индикаторы отключены в <Link to="/settings" className="text-teal hover:underline">настройках</Link>.
        </div>
      ) : (
        <>
          <ul className="divide-y divide-line">
            {risk.indicators.map((i) => (
              <li key={i.id} className="flex items-center justify-between gap-3 px-4 py-1.5 text-[13px]">
                <span className={cn("flex items-center gap-2", !i.triggered && "text-muted")}>
                  <span className={cn("size-1.5 rounded-full", i.triggered ? "bg-amber" : "bg-line-strong")} />
                  {i.label}
                </span>
                <span className="shrink-0 text-[11px] text-faint num">
                  {i.evidence} · вес {i.weight}
                </span>
              </li>
            ))}
          </ul>
          <Notice tone="info" className="m-3">
            {risk.disclaimer}
          </Notice>
        </>
      )}
    </Card>
  );
}
