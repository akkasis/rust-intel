import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { CheckCircle2, Circle, Loader2, LogIn, MessageSquare, Skull, Trash2, Wifi, WifiOff } from "lucide-react";
import { api } from "@/lib/api";
import { listen } from "@/lib/api/transport";
import type { RustPlusServer, RustPlusStatus } from "@/lib/types";
import { fmtRelative } from "@/lib/format";
import { Badge, Button, Card, CardHeader, EmptyState, ErrorState, LoadingBlock, Notice, PageHeader, StateDot, Switch, cn } from "@/components/ui";

function Step({ done, active, title, text }: { done: boolean; active?: boolean; title: string; text: React.ReactNode }) {
  return (
    <li className="flex gap-3">
      {done ? <CheckCircle2 className="mt-0.5 size-5 shrink-0 text-lime" /> : active ? <Loader2 className="mt-0.5 size-5 shrink-0 animate-spin text-teal" /> : <Circle className="mt-0.5 size-5 shrink-0 text-faint" />}
      <div>
        <div className={cn("font-medium", done && "text-muted")}>{title}</div>
        <div className="text-xs text-muted">{text}</div>
      </div>
    </li>
  );
}

export function RustPlusPage() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["rustplus"], queryFn: api.rustplusStatus, refetchInterval: 5000 });
  useEffect(() => {
    const p = listen("rid://rustplus", () => qc.invalidateQueries({ queryKey: ["rustplus"] }));
    return () => {
      p.then((u) => u());
    };
  }, [qc]);
  const login = useMutation({
    mutationFn: api.rustplusLogin,
    onSuccess: () => toast.info("Откроется окно входа Facepunch — войдите через Steam"),
    onError: (e) => toast.error(e.message),
  });
  const logout = useMutation({ mutationFn: api.rustplusLogout, onSuccess: () => qc.invalidateQueries({ queryKey: ["rustplus"] }) });
  if (q.isLoading) return <LoadingBlock rows={6} />;
  if (q.error) return <ErrorState error={q.error} retry={() => q.refetch()} />;
  const s = q.data!;
  const linked = s.device_registered && s.steam_linked;
  return (
    <div className="space-y-4">
      <PageHeader
        title="Rust+ · ваша команда"
        subtitle="Когда тиммейт погибает, программа пишет в командный чат игры: кто и в каком квадрате. Только ваша команда, через официальный Rust+."
        actions={
          linked ? (
            <Button variant="ghost" onClick={() => logout.mutate()}>
              Отключить Rust+
            </Button>
          ) : (
            <Button variant="primary" onClick={() => login.mutate()} loading={login.isPending} disabled={s.demo}>
              <LogIn className="size-4" /> Подключить Rust+ (вход через Steam)
            </Button>
          )
        }
      />
      {s.demo && <Notice tone="demo">Rust+ работает только в режиме Live. Ниже — пример того, как будет выглядеть подключённый сервер.</Notice>}
      <Card className="p-4">
        <ol className="space-y-3 text-[13px]">
          <Step done={s.device_registered} active={login.isPending && !s.device_registered} title="1. Регистрация устройства" text="Программа регистрируется для уведомлений Rust+, как телефон с приложением Rust+. Делается автоматически." />
          <Step
            done={s.steam_linked}
            title="2. Вход через Steam на сайте Facepunch"
            text={s.steam_linked ? `Связано со SteamID ${s.steam_id}` : "Нажмите «Подключить Rust+». Откроется официальная страница Facepunch — войдите через Steam. Программа не видит ваш пароль."}
          />
          <Step
            done={s.listener === "connected"}
            active={s.listener === "connecting" || s.listener === "reconnecting"}
            title="3. Приём спаривания"
            text={{ connected: "Готово к спариванию", connecting: "Подключаюсь…", reconnecting: "Переподключаюсь…", off: "Ожидает входа" }[s.listener] ?? s.listener}
          />
          <Step
            done={s.servers.length > 0}
            title="4. Спарьте сервер в игре"
            text={
              <>
                Зайдите на сервер → <b>Esc</b> → <b>Rust+</b> (справа) → <b>«Pair with server»</b>. Сервер появится ниже за несколько секунд, и программа начнёт следить за командой. На сервере должен быть включён Rust+.
              </>
            }
          />
        </ol>
      </Card>
      {linked && <Diagnostics s={s} />}
      {s.servers.length === 0 ? (
        <Card>
          <EmptyState icon={<MessageSquare className="size-7" />} title="Серверов пока нет" text="После шагов 1–3 нажмите «Pair with server» в игре." />
        </Card>
      ) : (
        s.servers.map((srv) => <ServerCard key={srv.id} s={srv} demo={s.demo} />)
      )}
      <Notice tone="info">
        Rust+ не сообщает причину смерти и убийцу — только кто погиб, где и когда. Программа должна быть запущена (можно в трее). Работает только для вашей команды на серверах, где вы сами нажали
        «Pair with server».
      </Notice>
    </div>
  );
}

function ServerCard({ s, demo }: { s: RustPlusServer; demo: boolean }) {
  const qc = useQueryClient();
  const upd = useMutation({
    mutationFn: (p: { enabled: boolean; chat: boolean; local: boolean }) => api.rustplusUpdateServer(s.id, p.enabled, p.chat, p.local),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["rustplus"] }),
    onError: (e) => toast.error(e.message),
  });
  const test = useMutation({ mutationFn: () => api.rustplusTestMessage(s.id), onSuccess: () => toast.success("Сообщение отправлено в командный чат"), onError: (e) => toast.error(e.message) });
  const del = useMutation({ mutationFn: () => api.rustplusRemoveServer(s.id), onSuccess: () => qc.invalidateQueries({ queryKey: ["rustplus"] }) });
  const st = s.status;
  const err = st.last_error ?? s.last_error;
  return (
    <Card>
      <CardHeader
        title={s.name}
        icon={st.connected ? <Wifi className="size-4 text-lime" /> : <WifiOff className="size-4 text-amber" />}
        subtitle={`${s.server_id ? "через сокет Facepunch" : `${s.ip}:${s.port}`} · ${st.connected ? `подключено${st.via === "direct" ? " напрямую" : ""}` : "не подключено"}${st.last_update ? ` · обновлено ${fmtRelative(st.last_update)}` : ""}`}
        actions={
          <>
            <Badge tone="lime" title="Сообщено о смертях с момента запуска">
              <Skull className="size-3" /> {st.deaths_reported}
            </Badge>
            <Button size="sm" onClick={() => test.mutate()} loading={test.isPending} disabled={!st.connected || demo}>
              Тест в чат
            </Button>
            <Button size="iconSm" variant="ghost" aria-label="Удалить сервер" onClick={() => del.mutate()} disabled={demo}>
              <Trash2 className="size-3.5" />
            </Button>
          </>
        }
      />
      <div className="grid gap-4 p-4 lg:grid-cols-[260px_minmax(0,1fr)]">
        <div className="space-y-2.5 text-[13px]">
          <label className="flex items-center justify-between gap-2">
            Следить за командой
            <Switch checked={s.enabled} onChange={(v) => upd.mutate({ enabled: v, chat: s.death_alerts_chat, local: s.death_alerts_local })} label="Следить за командой" disabled={demo} />
          </label>
          <label className="flex items-center justify-between gap-2">
            Писать о смертях в чат игры
            <Switch checked={s.death_alerts_chat} onChange={(v) => upd.mutate({ enabled: s.enabled, chat: v, local: s.death_alerts_local })} label="Писать в командный чат" disabled={demo} />
          </label>
          <label className="flex items-center justify-between gap-2">
            Уведомления Windows / Telegram
            <Switch checked={s.death_alerts_local} onChange={(v) => upd.mutate({ enabled: s.enabled, chat: s.death_alerts_chat, local: v })} label="Уведомления на ПК" disabled={demo} />
          </label>
          <div className="rounded-md border border-line bg-panel-2 p-2 text-xs text-muted">
            В чат придёт: <span className="text-fg">☠ Клещ умер в K14 (21:05)</span>
          </div>
          {err && <Notice tone="warning">{err}</Notice>}
        </div>
        <div>
          <div className="mb-1 text-[11px] font-medium tracking-wide text-faint uppercase">Команда</div>
          {st.members.length === 0 ? (
            <div className="text-xs text-muted">{st.connected ? "Загружаю состав команды…" : "Нет данных — сервер не подключён."}</div>
          ) : (
            <table className="w-full text-[13px]">
              <tbody className="divide-y divide-line">
                {st.members.map((m) => (
                  <tr key={m.steam_id}>
                    <td className="py-1.5">
                      <span className="flex items-center gap-2">
                        <StateDot state={m.online ? "online" : "offline"} />
                        {m.name}
                      </span>
                    </td>
                    <td className="py-1.5">{m.alive ? <Badge tone="teal">жив</Badge> : <Badge tone="danger">мёртв</Badge>}</td>
                    <td className="py-1.5 text-right text-muted num">{m.grid ?? "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </Card>
  );
}

function Diagnostics({ s }: { s: RustPlusStatus }) {
  const qc = useQueryClient();
  const selftest = useMutation({
    mutationFn: api.rustplusSelfTest,
    onSuccess: () => toast.info("Тестовое уведомление отправлено — ждём его в течение ~30 секунд"),
    onError: (e) => toast.error(e.message),
  });
  const rereg = useMutation({
    mutationFn: api.rustplusReregister,
    onSuccess: () => (toast.success("Устройство заново зарегистрировано в Facepunch — нажмите «Pair with server» ещё раз"), qc.invalidateQueries({ queryKey: ["rustplus"] })),
    onError: (e) => toast.error(e.message),
  });
  const st = s.selftest;
  const waiting = st && !st[1] && Date.now() - new Date(st[0]).getTime() < 60_000;
  const failed = st && !st[1] && !waiting;
  return (
    <Card>
      <CardHeader
        title="Диагностика уведомлений"
        subtitle="Если сервер не появляется после «Pair with server» — проверьте доставку и перерегистрируйтесь"
        actions={
          <>
            <Button size="sm" onClick={() => selftest.mutate()} loading={selftest.isPending}>
              Проверить доставку
            </Button>
            <Button size="sm" onClick={() => rereg.mutate()} loading={rereg.isPending}>
              Перерегистрировать в Facepunch
            </Button>
          </>
        }
      />
      <div className="grid gap-4 p-4 text-[13px] lg:grid-cols-[300px_minmax(0,1fr)]">
        <dl className="space-y-1.5 text-xs">
          <div className="flex justify-between gap-2">
            <dt className="text-muted">Самопроверка</dt>
            <dd>{!st ? "не запускалась" : st[1] ? <Badge tone="teal">доставка работает</Badge> : waiting ? <Badge tone="amber">ждём…</Badge> : <Badge tone="danger">не дошло</Badge>}</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-muted">Регистрация в Facepunch</dt>
            <dd>{s.diag.facepunch_error ? <span className="text-danger">{s.diag.facepunch_error}</span> : s.diag.facepunch_registered_at ? fmtRelative(s.diag.facepunch_registered_at) : "—"}</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-muted">Вход в поток уведомлений</dt>
            <dd>{s.diag.last_login_at ? fmtRelative(s.diag.last_login_at) : "—"}</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-muted">Переподключений</dt>
            <dd className="num">{s.diag.reconnects}</dd>
          </div>
          {s.diag.last_error && <div className="text-amber">Последняя ошибка: {s.diag.last_error}</div>}
          {failed && <Notice tone="error">Тестовое уведомление не дошло — перезапустите программу. Если не поможет, нажмите «Отключить Rust+» и подключитесь заново.</Notice>}
        </dl>
        <div>
          <div className="mb-1 text-[11px] font-medium tracking-wide text-faint uppercase">Последние уведомления</div>
          {s.push_log.length === 0 ? (
            <div className="text-xs text-muted">Пока ничего не приходило. Нажмите «Pair with server» в игре — запись появится здесь, даже если что-то пойдёт не так.</div>
          ) : (
            <ul className="divide-y divide-line">
              {s.push_log.map((p, i) => (
                <li key={i} className="py-1.5 text-xs">
                  <span className="text-faint num">{fmtRelative(p.at)}</span> · <span className="text-muted">{p.channel ?? "—"}</span>
                  {p.title && <span className="text-muted"> · {p.title}</span>}
                  <div>{p.result}</div>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </Card>
  );
}
