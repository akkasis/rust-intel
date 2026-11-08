import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { FlaskConical, ShieldCheck } from "lucide-react";
import { api } from "@/lib/api";
import { browserTimezone } from "@/lib/format";
import { Notice } from "@/components/ui";
import { Logo } from "@/layout/Logo";

export function FirstRun() {
  const qc = useQueryClient();
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const choose = async (mode: "demo" | "live") => {
    setBusy(mode);
    setErr(null);
    try {
      await api.chooseMode(mode, browserTimezone());
      await qc.invalidateQueries();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };
  return (
    <div className="flex h-full items-center justify-center p-6">
      <div className="w-[min(760px,100%)]">
        <div className="mb-6 flex items-center gap-3">
          <Logo size={44} />
          <div>
            <h1 className="font-display text-2xl font-semibold tracking-tight">Rust Intelligence Desktop</h1>
            <p className="text-sm text-muted">Локальный анализ публичной игровой активности Rust. Данные хранятся только на этом компьютере.</p>
          </div>
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          <button
            type="button"
            onClick={() => choose("demo")}
            disabled={!!busy}
            className="group rounded-xl border border-violet/30 bg-violet/6 p-5 text-left transition-colors hover:border-violet/60 focus-visible:border-violet"
          >
            <FlaskConical className="mb-3 size-6 text-violet" />
            <div className="font-display text-base font-semibold">Демо-режим</div>
            <p className="mt-1 text-xs text-muted">
              Вымышленные игроки, отряды и события — чтобы оценить интерфейс без ключей. Отдельная база, никогда не смешивается с реальными данными.
            </p>
            <div className="mt-3 text-xs text-violet">{busy === "demo" ? "Создаю демо-базу…" : "Открыть демо →"}</div>
          </button>
          <button
            type="button"
            onClick={() => choose("live")}
            disabled={!!busy}
            className="group rounded-xl border border-teal/30 bg-teal/6 p-5 text-left transition-colors hover:border-teal/60 focus-visible:border-teal"
          >
            <ShieldCheck className="mb-3 size-6 text-teal" />
            <div className="font-display text-base font-semibold">Реальный режим</div>
            <p className="mt-1 text-xs text-muted">
              Steam Web API (профиль, баны, «в Rust сейчас»), опционально BattleMetrics и battlemetrics.ru. Ключи вводятся в «Интеграциях» и хранятся в Windows Credential Manager.
            </p>
            <div className="mt-3 text-xs text-teal">{busy === "live" ? "Открываю…" : "Начать →"}</div>
          </button>
        </div>
        {err && (
          <Notice tone="error" className="mt-3">
            {err}
          </Notice>
        )}
        <p className="mt-5 text-[11px] leading-relaxed text-faint">
          RID показывает только то, что видно через документированные источники, и всегда указывает источник, свежесть и покрытие. Отсутствие игрока на наблюдаемых серверах ≠ отсутствие в Rust. Трекер работает, только пока приложение запущено. Режим можно сменить в настройках.
        </p>
      </div>
    </div>
  );
}
