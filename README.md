# Rust Intelligence Desktop (RID)

Локальное настольное приложение для Windows 10/11 для анализа **публичной игровой активности Rust**: профили и баны Steam,
наблюдаемые сессии, тепловые карты активности с учётом покрытия, отряды, Live-трекер с уведомлениями, каталог серверов.

RID не является инструментом слежки за людьми, не выносит вердиктов о читерстве и всегда показывает источник, свежесть и
покрытие данных. Отсутствие игрока на наблюдаемых серверах ≠ отсутствие в Rust.

## Скачать (Windows)

Страница релиза **latest** в GitHub (обновляется при каждом push в `main`):

* `RustIntelligence.exe` — портативная версия: скачать и запустить, установка не нужна;
* `RustIntelligence-setup.exe` — установщик с ярлыками.

Нужен Microsoft Edge WebView2 Runtime (встроен в Windows 11 и обновлённую Windows 10).
Подробная инструкция — внутри приложения, раздел «Инструкция».

## Стек

Tauri 2 · Rust (edition 2024, Tokio, reqwest, sqlx/SQLite WAL) · React 19 + TypeScript strict · Vite · React Router ·
TanStack Query · Zustand · Tailwind CSS 4 · Lucide · Apache ECharts. Секреты — Windows Credential Manager (`keyring`).

## Быстрый старт (разработка)

Требования: Node.js ≥ 22, Rust stable, на Windows — WebView2 (есть в Windows 11).

```bash
npm ci
```

```bash
npm run tauri:dev
```

Только интерфейс в браузере (демо-фикстуры, без Rust-ядра):

```bash
npm run dev
```

## Проверки

```bash
npm run typecheck && npm run lint && npm test
```

```bash
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

```bash
npx playwright test
```

Перегенерация фикстур UI из демо-сида Rust (после изменения DTO):

```bash
cd src-tauri && cargo test --lib export_ui_fixtures -- --ignored
```

## Сборка установщика Windows (NSIS x64)

На Windows:

```bash
npx tauri build --bundles nsis
```

Результат: `src-tauri/target/release/bundle/nsis/Rust Intelligence Desktop_0.1.0_x64-setup.exe`.

С другой ОС — через GitHub Actions: workflow `.github/workflows/ci.yml` (job `bundle`, `windows-latest`) собирает
установщик и публикует артефакт `rid-windows-x64-nsis`. Запуск: push в `main` или *Actions → CI → Run workflow*.

## Режимы

* **Демо** — вымышленные игроки (`DEMO·…`), серверы `[DEMO] …`, события. Отдельная БД `rid-demo.sqlite`, фиолетовая лента сверху.
* **Live** — только реальные источники. Без ключей приложение работает, но показывает `не настроено` вместо данных.

## Ключи и интеграции

| Источник | Что даёт | Где взять |
|---|---|---|
| Steam Web API key | профиль, баны, часы в Rust, «в Rust сейчас» | steamcommunity.com/dev/apikey |
| battlemetrics.ru key | каталог серверов, онлайн серверов, вайпы | battlemetrics.ru/api (бесплатно) |
| BattleMetrics.com token | сессии игроков на серверах, поиск по нику | требуется платная подписка (403 без неё) |
| Telegram bot token + chat ID | уведомления | @BotFather |

Ключи вводятся в разделе «Интеграции», хранятся в Windows Credential Manager, не попадают в SQLite, логи, экспорт и UI.

## Данные

`%APPDATA%\dev.rid.desktop\` — `rid-live.sqlite`, `rid-demo.sqlite`, `config.json` (без секретов).
Перед применением новых миграций создаётся резервная копия `*.pre-migration.*.sqlite`.

## Документация

* [docs/PROVIDER_FEASIBILITY.md](docs/PROVIDER_FEASIBILITY.md) — проверка источников и маппинг полей
* [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — архитектура и корректность аналитики
* [docs/ACCEPTANCE_CHECKLIST.md](docs/ACCEPTANCE_CHECKLIST.md) — чек-лист приёмки с доказательствами
* [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) — честные ограничения
