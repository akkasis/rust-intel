import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import { cva, type VariantProps } from "class-variance-authority";
import { AlertTriangle, CircleHelp, Inbox, Loader2, RefreshCw, X } from "lucide-react";
import { forwardRef, useEffect, useRef, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes } from "react";
import type { DataStatus, Provenance } from "@/lib/types";
import { PROVIDER_NAMES, fmtRelative, initials } from "@/lib/format";
import { openExternal } from "@/lib/external";

export function cn(...c: ClassValue[]) {
  return twMerge(clsx(c));
}

// ------------------------------------------------------------------ Button

const buttonV = cva(
  "inline-flex items-center justify-center gap-1.5 rounded-md font-medium whitespace-nowrap transition-colors disabled:opacity-45 disabled:pointer-events-none select-none",
  {
    variants: {
      variant: {
        primary: "bg-teal text-[#04201c] hover:brightness-110",
        secondary: "bg-raised text-fg border border-line hover:border-line-strong hover:bg-panel-2",
        ghost: "text-muted hover:text-fg hover:bg-raised",
        danger: "bg-danger/15 text-danger border border-danger/30 hover:bg-danger/25",
      },
      size: { sm: "h-7 px-2.5 text-xs", md: "h-8 px-3 text-[13px]", icon: "h-8 w-8", iconSm: "h-7 w-7" },
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof buttonV> {
  loading?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(({ className, variant, size, loading, children, disabled, ...p }, ref) => (
  <button ref={ref} className={cn(buttonV({ variant, size }), className)} disabled={disabled || loading} {...p}>
    {loading && <Loader2 className="size-3.5 animate-spin" aria-hidden />}
    {children}
  </button>
));
Button.displayName = "Button";

// -------------------------------------------------------------------- Card

export function Card({ className, children, ...p }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <section className={cn("rounded-lg border border-line bg-panel", className)} {...p}>
      {children}
    </section>
  );
}

export function CardHeader({ title, subtitle, actions, icon }: { title: ReactNode; subtitle?: ReactNode; actions?: ReactNode; icon?: ReactNode }) {
  return (
    <header className="flex items-start justify-between gap-3 border-b border-line px-4 py-2.5">
      <div className="flex min-w-0 items-center gap-2">
        {icon && <span className="text-muted">{icon}</span>}
        <div className="min-w-0">
          <h2 className="truncate font-display text-[13px] font-semibold tracking-wide text-fg">{title}</h2>
          {subtitle && <p className="truncate text-xs text-muted">{subtitle}</p>}
        </div>
      </div>
      {actions && <div className="flex shrink-0 items-center gap-1.5">{actions}</div>}
    </header>
  );
}

// ------------------------------------------------------------------- Badge

const badgeV = cva("inline-flex items-center gap-1 rounded px-1.5 py-px text-[11px] font-medium leading-4 whitespace-nowrap border", {
  variants: {
    tone: {
      neutral: "bg-raised text-muted border-line",
      teal: "bg-teal/12 text-teal border-teal/25",
      lime: "bg-lime/12 text-lime border-lime/25",
      amber: "bg-amber/12 text-amber border-amber/30",
      danger: "bg-danger/12 text-danger border-danger/30",
      violet: "bg-violet/12 text-violet border-violet/30",
    },
  },
  defaultVariants: { tone: "neutral" },
});

export function Badge({ tone, className, children, title }: VariantProps<typeof badgeV> & { className?: string; children: ReactNode; title?: string }) {
  return (
    <span className={cn(badgeV({ tone }), className)} title={title}>
      {children}
    </span>
  );
}

const STATUS_META: Record<DataStatus, { label: string; tone: VariantProps<typeof badgeV>["tone"]; hint: string }> = {
  verified: { label: "подтверждено", tone: "teal", hint: "Получено из источника и свежее" },
  stale: { label: "устарело", tone: "amber", hint: "Данные из кэша старше 24 часов" },
  partial: { label: "частично", tone: "amber", hint: "Источник отдал не всё (приватность, покрытие)" },
  unsupported: { label: "не поддерживается", tone: "neutral", hint: "Ни один источник не предоставляет эти данные" },
  not_configured: { label: "не настроено", tone: "neutral", hint: "Нужен ключ в разделе «Интеграции»" },
  error: { label: "ошибка", tone: "danger", hint: "Последний запрос к источнику завершился ошибкой" },
  demo: { label: "демо", tone: "violet", hint: "Вымышленные данные демо-режима" },
};

export function StatusBadge({ status }: { status: DataStatus }) {
  const m = STATUS_META[status] ?? STATUS_META.error;
  return (
    <Badge tone={m.tone} title={m.hint}>
      {m.label}
    </Badge>
  );
}

/** Чип происхождения: источник, свежесть, статус, ссылка. */
export function ProvenanceChip({ p, compact }: { p: Provenance; compact?: boolean }) {
  const src = PROVIDER_NAMES[p.source] ?? p.source;
  const title = [`Источник: ${src}`, p.fetched_at ? `Получено: ${fmtRelative(p.fetched_at)}` : "Время получения неизвестно", p.note ?? ""].filter(Boolean).join("\n");
  return (
    <span className="inline-flex items-center gap-1.5 text-[11px] text-muted" title={title}>
      <StatusBadge status={p.status} />
      {!compact && <span className="truncate">{src}</span>}
      {!compact && p.fetched_at && <span className="text-faint">· {fmtRelative(p.fetched_at)}</span>}
      {p.source_url && (
        <button type="button" className="text-teal hover:underline" onClick={() => openExternal(p.source_url!)} aria-label={`Открыть источник ${src}`}>
          ↗
        </button>
      )}
    </span>
  );
}

// ------------------------------------------------------------------ Inputs

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(({ className, ...p }, ref) => (
  <input
    ref={ref}
    className={cn(
      "h-8 w-full rounded-md border border-line bg-panel-2 px-2.5 text-[13px] text-fg placeholder:text-faint focus:border-teal/60 focus:outline-none",
      className,
    )}
    {...p}
  />
));
Input.displayName = "Input";

export function Select({ className, children, ...p }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select className={cn("h-8 rounded-md border border-line bg-panel-2 px-2 text-[13px] text-fg focus:border-teal/60 focus:outline-none", className)} {...p}>
      {children}
    </select>
  );
}

export function Switch({ checked, onChange, label, disabled, id }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean; id?: string }) {
  return (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative inline-flex h-[18px] w-8 shrink-0 items-center rounded-full border transition-colors disabled:opacity-40",
        checked ? "border-teal/60 bg-teal/80" : "border-line-strong bg-raised",
      )}
    >
      <span className={cn("inline-block size-3 rounded-full bg-fg transition-transform", checked ? "translate-x-[15px] bg-[#04201c]" : "translate-x-[3px]")} />
    </button>
  );
}

export function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: { value: T; label: string; disabled?: boolean }[]; onChange: (v: T) => void; label: string }) {
  return (
    <div role="radiogroup" aria-label={label} className="inline-flex rounded-md border border-line bg-panel-2 p-0.5">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          disabled={o.disabled}
          onClick={() => onChange(o.value)}
          className={cn(
            "h-6 rounded px-2.5 text-xs font-medium transition-colors disabled:opacity-40",
            value === o.value ? "bg-raised text-fg shadow-[inset_0_0_0_1px_var(--rid-line-strong)]" : "text-muted hover:text-fg",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Tabs<T extends string>({ value, tabs, onChange }: { value: T; tabs: { value: T; label: string; count?: number }[]; onChange: (v: T) => void }) {
  return (
    <div role="tablist" className="flex gap-1 border-b border-line">
      {tabs.map((t) => (
        <button
          key={t.value}
          role="tab"
          type="button"
          aria-selected={value === t.value}
          onClick={() => onChange(t.value)}
          className={cn(
            "-mb-px flex items-center gap-1.5 border-b-2 px-3 py-2 text-[13px] font-medium transition-colors",
            value === t.value ? "border-teal text-fg" : "border-transparent text-muted hover:text-fg",
          )}
        >
          {t.label}
          {t.count !== undefined && <span className="rounded bg-raised px-1 text-[10px] text-muted num">{t.count}</span>}
        </button>
      ))}
    </div>
  );
}

// ------------------------------------------------------------------ States

export function Skeleton({ className }: { className?: string }) {
  return <div className={cn("skeleton", className)} aria-hidden />;
}

export function LoadingBlock({ rows = 4 }: { rows?: number }) {
  return (
    <div className="space-y-2 p-4" role="status" aria-label="Загрузка">
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} className="h-6" />
      ))}
    </div>
  );
}

export function EmptyState({ title, text, action, icon }: { title: string; text?: ReactNode; action?: ReactNode; icon?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-10 text-center">
      <div className="text-faint">{icon ?? <Inbox className="size-7" />}</div>
      <div className="font-medium text-fg">{title}</div>
      {text && <div className="max-w-md text-xs text-muted">{text}</div>}
      {action}
    </div>
  );
}

export function ErrorState({ error, retry }: { error: unknown; retry?: () => void }) {
  const msg = error instanceof Error ? error.message : String(error);
  return (
    <div role="alert" className="flex flex-col items-center gap-2 px-6 py-8 text-center">
      <AlertTriangle className="size-6 text-danger" />
      <div className="font-medium">Не удалось загрузить</div>
      <div className="max-w-md text-xs text-muted">{msg}</div>
      {retry && (
        <Button size="sm" onClick={retry}>
          <RefreshCw className="size-3.5" /> Повторить
        </Button>
      )}
    </div>
  );
}

export function Notice({ tone = "info", children, className }: { tone?: "info" | "warning" | "error" | "demo"; children: ReactNode; className?: string }) {
  const map = {
    info: "border-teal/25 bg-teal/6 text-fg",
    warning: "border-amber/30 bg-amber/8 text-fg",
    error: "border-danger/30 bg-danger/8 text-fg",
    demo: "border-violet/30 bg-violet/8 text-fg",
  } as const;
  return <div className={cn("rounded-md border px-3 py-2 text-xs leading-relaxed", map[tone], className)}>{children}</div>;
}

export function Hint({ text }: { text: string }) {
  return (
    <span className="inline-flex cursor-help text-faint hover:text-muted" title={text} aria-label={text}>
      <CircleHelp className="size-3.5" />
    </span>
  );
}

export function Avatar({ name, url, size = 32, demo }: { name: string | null; url?: string | null; size?: number; demo?: boolean }) {
  const s = { width: size, height: size };
  if (url) return <img src={url} alt="" style={s} className="shrink-0 rounded-md border border-line object-cover" referrerPolicy="no-referrer" />;
  return (
    <span
      style={{ ...s, fontSize: size * 0.36 }}
      className={cn("inline-flex shrink-0 items-center justify-center rounded-md border font-display font-semibold", demo ? "border-violet/30 bg-violet/12 text-violet" : "border-line bg-raised text-muted")}
      aria-hidden
    >
      {initials(name)}
    </span>
  );
}

export function Kpi({ label, value, sub, tone, hint }: { label: string; value: ReactNode; sub?: ReactNode; tone?: "teal" | "lime" | "amber" | "danger"; hint?: string }) {
  const c = tone ? { teal: "text-teal", lime: "text-lime", amber: "text-amber", danger: "text-danger" }[tone] : "text-fg";
  return (
    <div className="min-w-0 rounded-lg border border-line bg-panel px-3.5 py-2.5">
      <div className="flex items-center gap-1 text-[11px] font-medium tracking-wide text-muted uppercase">
        {label}
        {hint && <Hint text={hint} />}
      </div>
      <div className={cn("mt-0.5 truncate font-display text-xl font-semibold num", c)}>{value}</div>
      {sub && <div className="truncate text-[11px] text-faint">{sub}</div>}
    </div>
  );
}

export function StateDot({ state }: { state: "online" | "offline" | "unknown" | null }) {
  const m = {
    online: ["bg-lime live-dot", "онлайн (наблюдается)"],
    offline: ["bg-faint", "не наблюдается онлайн"],
    unknown: ["hatch border border-line-strong", "неизвестно: нет данных от источника"],
  } as const;
  const [c, l] = m[state ?? "unknown"];
  return <span className={cn("inline-block size-2.5 shrink-0 rounded-full", c)} title={l} aria-label={l} role="img" />;
}

// ------------------------------------------------------------------ Dialog

export function Dialog({ open, onClose, title, children, footer, wide }: { open: boolean; onClose: () => void; title: string; children: ReactNode; footer?: ReactNode; wide?: boolean }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal?.();
    if (!open && d.open) d.close();
  }, [open]);
  return (
    <dialog
      ref={ref}
      onClose={onClose}
      onCancel={onClose}
      className={cn(
        "m-auto rounded-xl border border-line-strong bg-panel p-0 text-fg shadow-2xl backdrop:bg-black/60 backdrop:backdrop-blur-[2px]",
        wide ? "w-[min(760px,92vw)]" : "w-[min(480px,92vw)]",
      )}
      aria-label={title}
    >
      {open && (
        <>
          <div className="flex items-center justify-between border-b border-line px-4 py-3">
            <h2 className="font-display text-sm font-semibold">{title}</h2>
            <Button variant="ghost" size="iconSm" onClick={onClose} aria-label="Закрыть">
              <X className="size-4" />
            </Button>
          </div>
          <div className="max-h-[70vh] overflow-auto px-4 py-3">{children}</div>
          {footer && <div className="flex justify-end gap-2 border-t border-line px-4 py-3">{footer}</div>}
        </>
      )}
    </dialog>
  );
}

export function Field({ label, children, hint, htmlFor }: { label: string; children: ReactNode; hint?: ReactNode; htmlFor?: string }) {
  return (
    <div className="space-y-1">
      <label htmlFor={htmlFor} className="block text-xs font-medium text-muted">
        {label}
      </label>
      {children}
      {hint && <div className="text-[11px] text-faint">{hint}</div>}
    </div>
  );
}

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="mb-4 flex flex-wrap items-end justify-between gap-3">
      <div className="min-w-0">
        <h1 className="font-display text-xl font-semibold tracking-tight">{title}</h1>
        {subtitle && <div className="mt-0.5 text-xs text-muted">{subtitle}</div>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}
