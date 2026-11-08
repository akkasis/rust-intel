import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Database, Download, FlaskConical, HardDriveDownload, ShieldCheck, Upload } from "lucide-react";
import { api } from "@/lib/api";
import type { AppSettings } from "@/lib/types";
import { EVENT_TITLES, browserTimezone } from "@/lib/format";
import { pickOpenPath, pickSavePath } from "@/lib/files";
import { useAppInfo } from "@/app/hooks";
import { Badge, Button, Card, CardHeader, Dialog, ErrorState, Field, Input, LoadingBlock, Notice, PageHeader, Segmented, Select, Switch } from "@/components/ui";

function timezones(): string[] {
  try {
    const list = (Intl as unknown as { supportedValuesOf?: (k: string) => string[] }).supportedValuesOf?.("timeZone") ?? [];
    return list.length ? ["UTC", ...list] : ["UTC", "Europe/Moscow", "Europe/Berlin", "Europe/Kiev", "Asia/Yekaterinburg", "Asia/Novosibirsk"];
  } catch {
    return ["UTC", "Europe/Moscow"];
  }
}

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 px-4 py-2.5">
      <div className="min-w-0">
        <div className="text-[13px]">{label}</div>
        {hint && <div className="text-[11px] text-faint">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function SettingsPage() {
  const info = useAppInfo();
  if (info.isLoading) return <LoadingBlock rows={8} />;
  if (info.error) return <ErrorState error={info.error} retry={() => info.refetch()} />;
  return <SettingsForm key={JSON.stringify(info.data!.settings)} initial={info.data!.settings} />;
}

function SettingsForm({ initial }: { initial: AppSettings }) {
  const qc = useQueryClient();
  const info = useAppInfo();
  const [s, setS] = useState<AppSettings>(initial);
  const tzs = useMemo(timezones, []);
  const dirty = JSON.stringify(s) !== JSON.stringify(initial);
  const save = useMutation({
    mutationFn: (next: AppSettings) => api.updateSettings(next),
    onSuccess: () => {
      qc.invalidateQueries();
      toast.success("Настройки сохранены");
    },
    onError: (e) => toast.error(e.message),
  });
  const mode = useMutation({
    mutationFn: (m: "demo" | "live") => api.chooseMode(m),
    onSuccess: () => {
      qc.invalidateQueries();
      toast.success("Режим переключён");
    },
    onError: (e) => toast.error(e.message),
  });
  const set = <K extends keyof AppSettings>(k: K, v: AppSettings[K]) => setS((p) => ({ ...p, [k]: v }));
  const setN = <K extends keyof AppSettings["notifications"]>(k: K, v: AppSettings["notifications"][K]) => setS((p) => ({ ...p, notifications: { ...p.notifications, [k]: v } }));
  const current = info.data?.mode;

  return (
    <div className="space-y-4">
      <PageHeader
        title="Настройки"
        subtitle={`Версия ${info.data?.version} · схема БД v${info.data?.schema_version} · данные: ${info.data?.data_dir}`}
        actions={
          <>
            <Button variant="ghost" disabled={!dirty} onClick={() => setS(initial)}>
              Отменить
            </Button>
            <Button variant="primary" disabled={!dirty} loading={save.isPending} onClick={() => save.mutate(s)}>
              Сохранить
            </Button>
          </>
        }
      />
      <div className="grid gap-4 xl:grid-cols-2">
        <Card>
          <CardHeader title="Режим работы" />
          <div className="flex flex-wrap items-center gap-3 p-4">
            <Button variant={current === "demo" ? "primary" : "secondary"} onClick={() => mode.mutate("demo")} disabled={current === "demo"}>
              <FlaskConical className="size-4" /> Демо
            </Button>
            <Button variant={current === "live" ? "primary" : "secondary"} onClick={() => mode.mutate("live")} disabled={current === "live"}>
              <ShieldCheck className="size-4" /> Live
            </Button>
            <span className="text-xs text-muted">Демо и Live используют разные базы данных и никогда не смешиваются.</span>
          </div>
        </Card>

        <Card>
          <CardHeader title="Интерфейс" />
          <div className="divide-y divide-line">
            <Row label="Тема">
              <Segmented label="Тема" value={s.theme} onChange={(v) => set("theme", v)} options={[{ value: "dark", label: "Тёмная" }, { value: "light", label: "Светлая" }, { value: "system", label: "Системная" }]} />
            </Row>
            <Row label="Язык" hint="Архитектура готова к локализации">
              <Select value="ru" disabled aria-label="Язык">
                <option value="ru">Русский</option>
              </Select>
            </Row>
            <Row label="Часовой пояс" hint="Правила IANA с учётом летнего времени">
              <div className="flex items-center gap-2">
                <Select value={s.timezone} onChange={(e) => set("timezone", e.target.value)} aria-label="Часовой пояс" className="max-w-[220px]">
                  {tzs.map((t) => (
                    <option key={t} value={t}>
                      {t}
                    </option>
                  ))}
                </Select>
                <Button size="sm" variant="ghost" onClick={() => set("timezone", browserTimezone())}>
                  Системный
                </Button>
              </div>
            </Row>
            <Row label="Анимации">
              <Segmented label="Анимации" value={s.reduced_motion} onChange={(v) => set("reduced_motion", v)} options={[{ value: "system", label: "Как в системе" }, { value: "on", label: "Уменьшить" }, { value: "off", label: "Обычные" }]} />
            </Row>
          </div>
        </Card>

        <Card>
          <CardHeader title="Работа в фоне" subtitle="Наблюдение идёт, только пока приложение запущено" />
          <div className="divide-y divide-line">
            <Row label="Запускать вместе с Windows" hint="Стартует свёрнутым в трей">
              <Switch checked={s.autostart} onChange={(v) => set("autostart", v)} label="Автозапуск" />
            </Row>
            <Row label="Закрытие окна сворачивает в трей" hint="Иначе закрытие завершает приложение и опросы">
              <Switch checked={s.close_to_tray} onChange={(v) => set("close_to_tray", v)} label="Сворачивать в трей" />
            </Row>
            <Row label="Интервал опроса трекера" hint="Не чаще 60 с; ограничивается лимитами источников">
              <Select value={s.poll_interval_secs} onChange={(e) => set("poll_interval_secs", Number(e.target.value))} aria-label="Интервал опроса">
                {[60, 120, 180, 300, 600, 900].map((v) => (
                  <option key={v} value={v}>
                    {v / 60} мин
                  </option>
                ))}
              </Select>
            </Row>
            <Row label="Трекер на паузе">
              <Switch checked={s.tracking_paused} onChange={(v) => set("tracking_paused", v)} label="Пауза трекера" />
            </Row>
            <Row label="Хранить сырые наблюдения и события" hint="Старше — удаляются автоматически">
              <Select value={s.retention_days} onChange={(e) => set("retention_days", Number(e.target.value))} aria-label="Срок хранения">
                {[30, 90, 180, 365, 730].map((v) => (
                  <option key={v} value={v}>
                    {v} дней
                  </option>
                ))}
              </Select>
            </Row>
            <Row label="Индикаторы риска" hint="Объяснимая сумма публичных сигналов, без вердиктов">
              <Switch checked={s.risk_indicators} onChange={(v) => set("risk_indicators", v)} label="Индикаторы риска" />
            </Row>
          </div>
        </Card>

        <Card>
          <CardHeader title="Уведомления" subtitle="Только по игрокам с включённым колокольчиком; без дублей" />
          <div className="divide-y divide-line">
            <Row label="Уведомления Windows">
              <div className="flex items-center gap-2">
                <Button size="sm" variant="ghost" onClick={() => api.testNotification().then(() => toast.success("Отправлено"), (e) => toast.error(e.message))}>
                  Тест
                </Button>
                <Switch checked={s.notifications.windows} onChange={(v) => setN("windows", v)} label="Уведомления Windows" />
              </div>
            </Row>
            <Row label="Telegram" hint={s.notifications.telegram_chat_id ? `chat ${s.notifications.telegram_chat_id}` : "Укажите токен и chat ID в «Интеграциях»"}>
              <Switch checked={s.notifications.telegram} onChange={(v) => setN("telegram", v)} label="Telegram" />
            </Row>
            <Row label="Тихие часы" hint="В выбранном часовом поясе">
              <div className="flex items-center gap-1.5">
                <Input type="time" value={s.notifications.quiet_start ?? ""} onChange={(e) => setN("quiet_start", e.target.value || null)} className="w-28" aria-label="Начало тихих часов" />
                —
                <Input type="time" value={s.notifications.quiet_end ?? ""} onChange={(e) => setN("quiet_end", e.target.value || null)} className="w-28" aria-label="Конец тихих часов" />
              </div>
            </Row>
            <div className="grid grid-cols-1 gap-x-4 gap-y-1.5 px-4 py-3 sm:grid-cols-2">
              {Object.entries(EVENT_TITLES).map(([k, l]) => (
                <label key={k} className="flex items-center justify-between gap-2 text-xs">
                  <span className="text-muted">{l}</span>
                  <Switch checked={!!s.notifications.events[k]} onChange={(v) => setN("events", { ...s.notifications.events, [k]: v })} label={l} />
                </label>
              ))}
            </div>
          </div>
        </Card>
      </div>
      <DataCard />
    </div>
  );
}

function DataCard() {
  const qc = useQueryClient();
  const [purge, setPurge] = useState(false);
  const [confirm, setConfirm] = useState("");
  const [diagOpen, setDiagOpen] = useState(false);
  const diag = useQuery({ queryKey: ["diagnostics"], queryFn: api.diagnostics, enabled: diagOpen });
  const stamp = new Date().toISOString().slice(0, 10);
  const run = async (fn: () => Promise<unknown>, ok: string) => {
    try {
      const r = await fn();
      if (r !== undefined && r !== null && r !== false) toast.success(ok);
      qc.invalidateQueries();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
  };
  const exportAs = (format: "json" | "csv_sessions" | "csv_events") =>
    run(async () => {
      const ext = format === "json" ? "json" : "csv";
      const p = await pickSavePath(`rid-${format}-${stamp}.${ext}`, ext, format === "json" ? "JSON" : "CSV");
      return p ? api.exportData(p, format) : null;
    }, "Экспорт выполнен (секреты не экспортируются)");
  const del = useMutation({
    mutationFn: () => api.purgeAll(confirm),
    onSuccess: () => {
      setPurge(false);
      setConfirm("");
      qc.invalidateQueries();
      toast.success("Локальные данные удалены");
    },
    onError: (e) => toast.error(e.message),
  });
  return (
    <Card>
      <CardHeader title="Данные" icon={<Database className="size-4" />} subtitle="Экспорт без секретов, резервные копии SQLite, проверка целостности" />
      <div className="flex flex-wrap gap-2 p-4">
        <Button onClick={() => exportAs("json")}>
          <Download className="size-3.5" /> Экспорт JSON
        </Button>
        <Button onClick={() => exportAs("csv_sessions")}>
          <Download className="size-3.5" /> Сессии CSV
        </Button>
        <Button onClick={() => exportAs("csv_events")}>
          <Download className="size-3.5" /> События CSV
        </Button>
        <Button
          onClick={() =>
            run(async () => {
              const p = await pickOpenPath(["json"], "Экспорт RID");
              if (!p) return null;
              const r = await api.importData(p);
              toast.info(`Игроков: ${r.players}, сессий: ${r.sessions}, пропущено (другой режим): ${r.skipped_demo}`);
              return r;
            }, "Импорт завершён")
          }
        >
          <Upload className="size-3.5" /> Импорт JSON
        </Button>
        <span className="mx-1 w-px bg-line" />
        <Button onClick={() => run(async () => { const p = await pickSavePath(`rid-backup-${stamp}.sqlite`, "sqlite", "SQLite"); return p ? api.backup(p) : null; }, "Резервная копия создана")}>
          <HardDriveDownload className="size-3.5" /> Резервная копия
        </Button>
        <Button
          onClick={() =>
            run(async () => {
              const p = await pickOpenPath(["sqlite", "db"], "Резервная копия RID");
              if (!p) return null;
              if (!window.confirm("Заменить текущую базу резервной копией? Текущая база будет сохранена рядом как before-restore.")) return null;
              await api.restore(p);
              return true;
            }, "База восстановлена")
          }
        >
          Восстановить
        </Button>
        <Button variant="ghost" onClick={() => setDiagOpen(true)}>
          Диагностика
        </Button>
        <Button variant="danger" className="ml-auto" onClick={() => setPurge(true)}>
          Удалить все данные
        </Button>
      </div>
      <Dialog open={diagOpen} onClose={() => setDiagOpen(false)} title="Диагностика" wide>
        {diag.isLoading && <LoadingBlock />}
        {diag.error && <ErrorState error={diag.error} />}
        {diag.data && (
          <div className="space-y-3 text-[13px]">
            <div className="flex flex-wrap gap-2">
              <Badge tone={diag.data.integrity === "ok" ? "teal" : "danger"}>integrity_check: {diag.data.integrity}</Badge>
              <Badge>схема v{diag.data.schema_version}</Badge>
              <Badge>{(diag.data.db_size_bytes / 1024 / 1024).toFixed(1)} МБ</Badge>
              <Badge>секреты: {diag.data.secret_backend === "os_keyring" ? "системное хранилище" : "только память"}</Badge>
            </div>
            <div className="text-xs text-muted break-all">{diag.data.db_path}</div>
            <table className="w-full text-xs">
              <tbody className="divide-y divide-line">
                {diag.data.counts.map(([k, v]) => (
                  <tr key={k}>
                    <td className="py-1">{k}</td>
                    <td className="py-1 text-right num">{v}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <pre className="max-h-48 overflow-auto rounded bg-panel-2 p-2 text-[11px] text-muted">{JSON.stringify({ tracker: diag.data.tracker, health: diag.data.health }, null, 2)}</pre>
          </div>
        )}
      </Dialog>
      <Dialog
        open={purge}
        onClose={() => setPurge(false)}
        title="Удалить все локальные данные?"
        footer={
          <>
            <Button onClick={() => setPurge(false)}>Отмена</Button>
            <Button variant="danger" disabled={confirm !== "УДАЛИТЬ"} loading={del.isPending} onClick={() => del.mutate()}>
              Удалить безвозвратно
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Notice tone="error">Будут удалены игроки, наблюдения, сессии, события, отряды и кэш серверов текущего режима. Ключи в системном хранилище не затрагиваются. Действие необратимо.</Notice>
          <Field label="Введите УДАЛИТЬ для подтверждения" htmlFor="purge-confirm">
            <Input id="purge-confirm" value={confirm} onChange={(e) => setConfirm(e.target.value)} />
          </Field>
        </div>
      </Dialog>
    </Card>
  );
}
