import { useEffect, useRef } from "react";
import * as echarts from "echarts/core";
import { BarChart, LineChart } from "echarts/charts";
import { DataZoomComponent, GridComponent, LegendComponent, MarkAreaComponent, MarkLineComponent, TooltipComponent } from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import type { EChartsCoreOption } from "echarts/core";

echarts.use([BarChart, LineChart, GridComponent, TooltipComponent, LegendComponent, DataZoomComponent, MarkAreaComponent, MarkLineComponent, CanvasRenderer]);

export function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
}

export function chartTheme() {
  return {
    fg: cssVar("--rid-fg"),
    muted: cssVar("--rid-muted"),
    faint: cssVar("--rid-faint"),
    line: cssVar("--rid-line"),
    teal: cssVar("--rid-teal"),
    lime: cssVar("--rid-lime"),
    amber: cssVar("--rid-amber"),
    danger: cssVar("--rid-danger"),
    violet: cssVar("--rid-violet"),
    unknown: cssVar("--rid-hatch"),
    panel: cssVar("--rid-panel-2"),
  };
}

export function baseOption(): EChartsCoreOption {
  const t = chartTheme();
  return {
    animationDuration: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : 300,
    textStyle: { color: t.muted, fontFamily: "inherit", fontSize: 11 },
    grid: { left: 40, right: 12, top: 24, bottom: 28, containLabel: false },
    tooltip: {
      trigger: "axis",
      backgroundColor: t.panel,
      borderColor: t.line,
      textStyle: { color: t.fg, fontSize: 12 },
      axisPointer: { lineStyle: { color: t.faint } },
    },
  };
}

export function EChart({ option, height = 220, label }: { option: EChartsCoreOption; height?: number; label: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const inst = useRef<echarts.ECharts | null>(null);
  useEffect(() => {
    if (!ref.current) return;
    const c = echarts.init(ref.current, undefined, { renderer: "canvas" });
    inst.current = c;
    const ro = new ResizeObserver(() => c.resize());
    ro.observe(ref.current);
    return () => {
      ro.disconnect();
      c.dispose();
      inst.current = null;
    };
  }, []);
  useEffect(() => {
    inst.current?.setOption(option, true);
  }, [option]);
  return <div ref={ref} style={{ height }} role="img" aria-label={label} className="w-full" />;
}
