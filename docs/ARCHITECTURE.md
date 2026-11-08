# Архитектура

```text
src/                         React/Vite UI
  app/        App, роутер (hash), first-run, hooks, Zustand (UTC-переключатель, палитра, сайдбар)
  layout/     AppShell (сайдбар, шапка, демо-лента), CommandPalette (Ctrl+K), Logo
  features/   dashboard, search, players (список, профиль), activity, squads, tracker, bans, servers, integrations, settings
  components/ ui (кнопки, бейджи, состояния, диалоги), charts (ECharts, Heatmap 7×24), domain (события, здоровье)
  lib/        api (типизированные вызовы + mock-транспорт), types, format, validation, external, files
src-tauri/src/
  commands/       узкая граница Tauri-команд (≈45 команд), секреты наружу не отдаются
  domain/         ids (SteamId64 ≠ BattleMetricsPlayerId), analytics, bans/risk, provenance, tracking
  providers/      steam, battlemetrics, bmru (battlemetrics.ru), demo, error, трейты и capability-матрица
  services/       player, analytics, tracker (планировщик), server, notifications, export
  persistence/    db (WAL, миграции, бэкап перед миграцией, VACUUM INTO), models, repo (параметризованные запросы)
  infrastructure/ http (allowlist, лимиты, ретраи, Retry-After), secrets (Credential Manager), redact, clock
  migrations/     версионированные SQL-миграции
tests/fixtures/   санитизированные ответы API для контрактных тестов
```

## Ключевые решения
* **Демо и Live — разные файлы БД.** Демо-SteamID лежат в диапазоне account id ≥ 4.1 млрд (ещё не выданы Steam),
  имена с префиксом `DEMO·`; импорт не смешивает режимы.
* **SteamID64 — строка** в JSON и TS; в TS-валидации — `BigInt`.
* **Ошибки провайдеров типизированы** (`not_configured / forbidden / rate_limited / unavailable / unsupported / not_found / timeout / invalid_data`),
  сбой источника (`is_outage`) никогда не превращается в «офлайн».
* **HTTP**: только https и хосты из allowlist, без редиректов, лимит 1 rps на провайдера, 2 ретрая с экспоненциальной
  задержкой и джиттером, `Retry-After` ≤ 10 с, ответ ≤ 8 МБ, ключи вырезаются из логов.
* **CSP** строгий (`default-src 'self'`, изображения только с CDN Steam), `freezePrototype`, минимальные capabilities,
  внешние ссылки открываются только для allowlist-хостов.

## Корректность аналитики
1. **Наблюдение ≠ сессия.** `observations` — сырые опросы (`online/offline/unknown`); `sessions` — восстановленные интервалы
   с границами `[start_lower, start_upper]` и `[end_lower, end_upper]` (переход случился между опросами).
2. **Покрытие** (`coverage_windows`) — знаменатель. Интервал между опросами засчитывается, только если разрыв ≤ 2.5 × интервала
   и предыдущий опрос был успешным. Окна неопределённости переходов исключены и из числителя, и из знаменателя.
3. **Heatmap 7×24**: 15-минутная UTC-сетка (все смещения поясов кратны 15 мин), локальный час по правилам IANA (chrono-tz),
   переходы DST корректны (тест `dst_transition…`). Ячейка: `known` / `insufficient` (< 30 мин) / `unknown` (0 — штриховка).
4. **Отряды**: sweep-line по событиям границ интервалов с битовыми масками онлайн/известности участников.
   `all_observed_offline` — только если известны **все**; иначе `unknown`. Без перемножения «вероятностей».
5. **События**: только для наблюдённых переходов между двумя последовательными успешными опросами; `dedup_key UNIQUE`
   (идемпотентность), отдельные события `source_unavailable/source_restored`.
6. **Типичные офлайн-часы** — только при достаточной повторяемости, иначе `insufficient_data`.
