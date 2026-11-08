import { useMemo, useState } from "react";
import type { HeatCell } from "@/lib/types";
import { WEEKDAYS, fmtDuration, fmtPct } from "@/lib/format";
import { cn } from "@/components/ui";

/** Тепловая карта 7×24. Неизвестные ячейки — штриховка, а не «0%». */
export function ActivityHeatmap({ cells, highlight, onCell }: { cells: HeatCell[]; highlight?: Set<string>; onCell?: (c: HeatCell) => void }) {
  const [hover, setHover] = useState<HeatCell | null>(null);
  const grid = useMemo(() => {
    const g: (HeatCell | undefined)[][] = Array.from({ length: 7 }, () => Array(24));
    for (const c of cells) g[c.weekday]![c.hour] = c;
    return g;
  }, [cells]);
  return (
    <div>
      <div className="grid gap-[3px]" style={{ gridTemplateColumns: "28px repeat(24, minmax(0, 1fr))" }} role="grid" aria-label="Активность по дням недели и часам">
        <div />
        {Array.from({ length: 24 }, (_, h) => (
          <div key={h} className="text-center text-[10px] text-faint num" aria-hidden>
            {h % 3 === 0 ? String(h).padStart(2, "0") : ""}
          </div>
        ))}
        {grid.map((row, w) => (
          <div key={w} className="contents" role="row">
            <div className="flex items-center text-[11px] text-muted">{WEEKDAYS[w]}</div>
            {row.map((c, h) => {
              const key = `${w}:${h}`;
              const label = c ? cellLabel(c) : "нет данных";
              const r = c?.ratio ?? null;
              const style = c?.state === "known" && r !== null ? { backgroundColor: `color-mix(in oklab, var(--rid-teal) ${Math.round(8 + r * 92)}%, var(--rid-panel-2))` } : undefined;
              return (
                <button
                  key={h}
                  type="button"
                  role="gridcell"
                  aria-label={`${WEEKDAYS[w]} ${String(h).padStart(2, "0")}:00 — ${label}`}
                  onMouseEnter={() => c && setHover(c)}
                  onFocus={() => c && setHover(c)}
                  onClick={() => c && onCell?.(c)}
                  style={style}
                  className={cn(
                    "aspect-[2] min-h-[12px] rounded-[3px] border border-transparent hover:border-fg/60 focus-visible:border-fg",
                    !c || c.state === "unknown" ? "hatch" : c.state === "insufficient" ? "bg-unknown opacity-80 [background-image:radial-gradient(var(--rid-hatch)_1px,transparent_1.5px)] [background-size:5px_5px]" : "",
                    highlight?.has(key) && "outline outline-1 outline-amber/80",
                  )}
                />
              );
            })}
          </div>
        ))}
      </div>
      <div className="mt-2 flex flex-wrap items-center justify-between gap-3 text-[11px] text-muted">
        <div className="flex items-center gap-3">
          <span className="flex items-center gap-1">
            <span className="inline-block h-2.5 w-14 rounded-sm" style={{ background: "linear-gradient(90deg, color-mix(in oklab, var(--rid-teal) 8%, var(--rid-panel-2)), var(--rid-teal))" }} />
            доля онлайн от наблюдаемого времени
          </span>
          <span className="flex items-center gap-1">
            <span className="hatch inline-block size-2.5 rounded-sm" /> нет наблюдений
          </span>
          <span className="flex items-center gap-1">
            <span className="inline-block size-2.5 rounded-sm bg-unknown [background-image:radial-gradient(var(--rid-hatch)_1px,transparent_1.5px)] [background-size:5px_5px]" /> мало данных
          </span>
          {highlight && highlight.size > 0 && (
            <span className="flex items-center gap-1">
              <span className="inline-block size-2.5 rounded-sm outline outline-1 outline-amber" /> типичные офлайн-часы
            </span>
          )}
        </div>
        <div className="min-h-4 num" aria-live="polite">
          {hover ? `${WEEKDAYS[hover.weekday]} ${String(hover.hour).padStart(2, "0")}:00 · ${cellLabel(hover)}` : "Наведите на ячейку: доля, знаменатель, число дней"}
        </div>
      </div>
    </div>
  );
}

function cellLabel(c: HeatCell): string {
  if (c.state === "unknown") return "нет наблюдений (неизвестно, не «офлайн»)";
  const base = `онлайн ${fmtDuration(c.online_secs)} из ${fmtDuration(c.known_secs)} наблюдаемых · дней: ${c.sample_days}`;
  if (c.state === "insufficient") return `мало данных — ${base}`;
  return `${fmtPct(c.ratio)} · ${base}`;
}
