import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { ProvenanceChip, StatusBadge, Switch } from "./index";
import { ActivityHeatmap } from "@/components/charts/Heatmap";
import type { HeatCell } from "@/lib/types";

describe("UI", () => {
  it("StatusBadge различает статусы происхождения", () => {
    render(<StatusBadge status="not_configured" />);
    expect(screen.getByText("не настроено")).toBeInTheDocument();
  });
  it("ProvenanceChip показывает источник и не даёт ссылку на чужой хост", () => {
    render(<ProvenanceChip p={{ source: "steam", fetched_at: null, coverage: null, status: "verified", source_url: null, note: null }} />);
    expect(screen.getByText("Steam Web API")).toBeInTheDocument();
    expect(screen.queryByRole("button")).toBeNull();
  });
  it("Switch доступен с клавиатуры (role=switch)", () => {
    const fn = vi.fn();
    render(<Switch checked={false} onChange={fn} label="Тест" />);
    fireEvent.click(screen.getByRole("switch", { name: "Тест" }));
    expect(fn).toHaveBeenCalledWith(true);
  });
  it("Heatmap: неизвестная ячейка подписана как «неизвестно», а не 0%", () => {
    const cells: HeatCell[] = Array.from({ length: 168 }, (_, i) => ({
      weekday: Math.floor(i / 24),
      hour: i % 24,
      online_secs: i === 0 ? 1800 : 0,
      known_secs: i === 0 ? 3600 : 0,
      sample_days: i === 0 ? 3 : 0,
      ratio: i === 0 ? 0.5 : null,
      state: i === 0 ? "known" : "unknown",
    }));
    render(<ActivityHeatmap cells={cells} />);
    expect(screen.getAllByRole("gridcell")).toHaveLength(168);
    expect(screen.getByRole("gridcell", { name: /Пн 00:00 — 50%/ })).toBeInTheDocument();
    expect(screen.getByRole("gridcell", { name: /Пн 01:00 — нет наблюдений/ })).toBeInTheDocument();
  });
});
