import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Check, ExternalLink, KeyRound, Minus, PlugZap } from "lucide-react";
import { api } from "@/lib/api";
import type { IntegrationStatus, SecretKey } from "@/lib/types";
import { fmtRelative } from "@/lib/format";
import { openExternal } from "@/lib/external";
import { useAppInfo } from "@/app/hooks";
import { Badge, Button, Card, CardHeader, ErrorState, Input, LoadingBlock, Notice, PageHeader } from "@/components/ui";
import { HealthBadge } from "@/components/domain";

const ACCESS: Record<IntegrationStatus["access"], { label: string; tone: "teal" | "amber" | "danger" | "neutral" }> = {
  verified: { label: "подтверждён запросом", tone: "teal" },
  not_tested: { label: "не проверен", tone: "neutral" },
  denied: { label: "доступ запрещён", tone: "danger" },
  unsupported: { label: "нет адаптера", tone: "neutral" },
};

const HOWTO: Record<string, string> = {
  steam: "Ключ выдаётся бесплатно на steamcommunity.com/dev/apikey (нужен аккаунт Steam с покупками).",
  battlemetrics: "Токен создаётся в настройках разработчика BattleMetrics. По состоянию на ноябрь 2026 API требует платную подписку — без неё ответ 403.",
  battlemetrics_ru: "Бесплатный ключ вида bm_live_… на battlemetrics.ru/api. Только серверные данные.",
};

export function IntegrationsPage() {
  const info = useAppInfo();
  const q = useQuery({ queryKey: ["integrations"], queryFn: api.integrations });
  return (
    <div className="space-y-4">
      <PageHeader title="Интеграции" subtitle="Ключи хранятся в системном хранилище (Windows Credential Manager) и никогда не возвращаются в интерфейс." />
      {info.data?.secret_backend === "memory_only" && (
        <Notice tone="warning">Системное хранилище секретов недоступно: ключи будут действовать только до закрытия приложения и не сохранятся на диск.</Notice>
      )}
      {info.data?.mode === "demo" && <Notice tone="demo">Демо-режим не обращается к сети. Ключи можно сохранить заранее — они понадобятся в режиме Live.</Notice>}
      {q.isLoading && <LoadingBlock rows={6} />}
      {q.error && <ErrorState error={q.error} retry={() => q.refetch()} />}
      <div className="grid gap-4 2xl:grid-cols-2">
        {q.data?.filter((s) => s.descriptor.id !== "telegram").map((s) => <ProviderCard key={s.descriptor.id} s={s} />)}
        <TelegramCard status={q.data?.find((s) => s.descriptor.id === "telegram")} />
      </div>
    </div>
  );
}

function SecretForm({ secretKey, configured, masked, placeholder }: { secretKey: SecretKey; configured: boolean; masked: string | null; placeholder: string }) {
  const qc = useQueryClient();
  const [v, setV] = useState("");
  const save = useMutation({
    mutationFn: () => api.setSecret(secretKey, v),
    onSuccess: (b) => {
      setV("");
      qc.invalidateQueries({ queryKey: ["integrations"] });
      toast.success(b === "os_keyring" ? "Сохранено в системном хранилище" : "Сохранено только в памяти до закрытия");
    },
    onError: (e) => toast.error(e.message),
  });
  const clear = useMutation({ mutationFn: () => api.clearSecret(secretKey), onSuccess: () => (qc.invalidateQueries({ queryKey: ["integrations"] }), toast.success("Ключ удалён")) });
  return (
    <form
      className="flex gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (v.trim()) save.mutate();
      }}
    >
      <div className="relative flex-1">
        <KeyRound className="pointer-events-none absolute top-2 left-2.5 size-4 text-faint" />
        <Input type="password" autoComplete="off" spellCheck={false} value={v} onChange={(e) => setV(e.target.value)} placeholder={configured ? `Сохранён ${masked ?? ""} — введите новый для замены` : placeholder} className="pl-8" aria-label="Секрет" maxLength={512} />
      </div>
      <Button type="submit" variant="primary" disabled={!v.trim()} loading={save.isPending}>
        Сохранить
      </Button>
      {configured && (
        <Button type="button" variant="ghost" onClick={() => clear.mutate()}>
          Удалить
        </Button>
      )}
    </form>
  );
}

function ProviderCard({ s }: { s: IntegrationStatus }) {
  const qc = useQueryClient();
  const d = s.descriptor;
  const test = useMutation({
    mutationFn: () => api.testIntegration(d.id),
    onSuccess: (m) => (toast.success(m), qc.invalidateQueries({ queryKey: ["integrations"] })),
    onError: (e) => (toast.error(e.message), qc.invalidateQueries({ queryKey: ["integrations"] })),
  });
  const a = ACCESS[s.access];
  return (
    <Card>
      <CardHeader
        title={d.name}
        subtitle={d.terms_note}
        icon={<PlugZap className="size-4" />}
        actions={
          <>
            <Badge tone={a.tone}>{a.label}</Badge>
            {d.credential && <Badge tone={s.configured ? "teal" : "neutral"}>{s.configured ? "ключ задан" : "не настроен"}</Badge>}
          </>
        }
      />
      <div className="space-y-3 p-4">
        {HOWTO[d.id] && <p className="text-xs text-muted">{HOWTO[d.id]}</p>}
        {d.credential && <SecretForm secretKey={d.credential as SecretKey} configured={s.configured} masked={s.masked} placeholder="Вставьте ключ" />}
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted">
          {d.credential && (
            <Button size="sm" onClick={() => test.mutate()} loading={test.isPending} disabled={!s.configured}>
              Проверить подключение
            </Button>
          )}
          <HealthBadge h={s.health ?? undefined} />
          {s.health?.last_success_at && <span>успех {fmtRelative(s.health.last_success_at)}</span>}
          {s.health?.last_error && <span className="text-amber" title={s.health.last_error}>ошибка: {s.health.last_error_kind}</span>}
          {d.docs_url && (
            <button type="button" className="ml-auto flex items-center gap-1 text-teal hover:underline" onClick={() => openExternal(d.docs_url)}>
              Документация <ExternalLink className="size-3" />
            </button>
          )}
        </div>
        <table className="w-full text-xs">
          <thead className="text-left text-[10px] tracking-wide text-faint uppercase">
            <tr>
              <th className="py-1 font-medium">Возможность</th>
              <th className="py-1 font-medium">Требует</th>
              <th className="py-1 font-medium">Примечание</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {d.capabilities.map((c) => (
              <tr key={c.capability}>
                <td className="py-1.5 pr-2 whitespace-nowrap">
                  <span className="flex items-center gap-1">
                    {c.supported ? <Check className="size-3 text-teal" /> : <Minus className="size-3 text-faint" />}
                    <code className="text-[11px]">{c.capability}</code>
                  </span>
                </td>
                <td className="py-1.5 pr-2 text-muted">{c.requires || "—"}</td>
                <td className="py-1.5 text-muted">{c.note}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Card>
  );
}

function TelegramCard({ status }: { status: IntegrationStatus | undefined }) {
  const qc = useQueryClient();
  const info = useAppInfo();
  const test = useMutation({ mutationFn: () => api.testIntegration("telegram"), onSuccess: (m) => toast.success(m), onError: (e) => toast.error(e.message) });
  const [chat, setChat] = useState(info.data?.settings.notifications.telegram_chat_id ?? "");
  const saveChat = useMutation({
    mutationFn: () => {
      const s = info.data!.settings;
      return api.updateSettings({ ...s, notifications: { ...s.notifications, telegram_chat_id: chat.trim() || null } });
    },
    onSuccess: () => (qc.invalidateQueries({ queryKey: ["app-info"] }), toast.success("Chat ID сохранён")),
    onError: (e) => toast.error(e.message),
  });
  return (
    <Card>
      <CardHeader
        title="Telegram-уведомления"
        subtitle="Отправка с этого компьютера через вашего бота, пока приложение запущено"
        icon={<PlugZap className="size-4" />}
        actions={<Badge tone={status?.configured ? "teal" : "neutral"}>{status?.configured ? "токен задан" : "не настроен"}</Badge>}
      />
      <div className="space-y-3 p-4">
        <p className="text-xs text-muted">
          Создайте бота у @BotFather, отправьте ему сообщение и укажите свой chat ID. Сообщения содержат только видимую игровую активность (ник, событие, время, сервер). Круглосуточного мониторинга без запущенного ПК нет.
        </p>
        <SecretForm secretKey="telegram_bot_token" configured={!!status?.configured} masked={status?.masked ?? null} placeholder="Токен бота 123456:ABC…" />
        <div className="flex gap-2">
          <Input value={chat} onChange={(e) => setChat(e.target.value)} placeholder="Chat ID, например 123456789" aria-label="Telegram chat ID" maxLength={64} />
          <Button onClick={() => saveChat.mutate()} loading={saveChat.isPending} disabled={!info.data}>
            Сохранить
          </Button>
          <Button onClick={() => test.mutate()} loading={test.isPending}>
            Тест
          </Button>
        </div>
        <p className="text-[11px] text-faint">Включение и тихие часы — в «Настройках» → «Уведомления».</p>
      </div>
    </Card>
  );
}
