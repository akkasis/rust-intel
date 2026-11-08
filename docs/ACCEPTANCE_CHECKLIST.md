# Чек-лист приёмки

> Файлы `DATA_CONTRACTS.md` и `ACCEPTANCE_CHECKLIST.md` из исходного задания не были переданы. Этот чек-лист составлен
> по `MASTER_PROMPT.md` и `PRODUCT_SPEC.md`. Отметки: ✅ проверено (доказательство), 🟡 реализовано, но проверено только
> компиляцией/частично, ❌ не сделано.

## P0
| Пункт | Статус | Доказательство |
|---|---|---|
| Приложение запускается, демо с вымышленными данными | ✅ | `providers::demo::tests::seed_is_idempotent_and_marked_demo`; UI в браузере (скриншоты в сессии); e2e `демо-лента и обзор` |
| Парсинг SteamID64 / URL / vanity / Steam2 / Steam3 | ✅ | `domain::ids::tests::*` (6), `src/lib/format.test.ts` (зеркало TS, BigInt) |
| Поиск → профиль → SQLite | ✅ | `player_service::tests::demo_search_and_profile`; e2e `поиск по SteamID64…` |
| Steam профиль/баны (адаптер) | 🟡 | контрактные тесты `providers::steam::tests::*`; реальный ключ не проверялся |
| Статусы источников и provenance | ✅ | `live_without_key_is_not_configured_not_fake`; ProvenanceChip/StatusBadge (`ui.test.tsx`) |
| Визуализация активности с неизвестными данными | ✅ | `analytics::tests::missing_hours_are_unknown_not_offline`; `ui.test.tsx` Heatmap; e2e `активность…` |
| Навигация, настройки | ✅ | 10 экранов, роутер; e2e переходов |
| Сборка Tauri-приложения | ✅ | `npx tauri build --bundles app` на macOS: release-бинарник 12 МБ, запуск без паник 10 с (smoke). Mock-фикстуры в бандл не попадают |
| Windows release CI + NSIS | 🟡 | `.github/workflows/ci.yml` (windows-latest, nsis, smoke-тест, артефакт); локально не запускалось |
| Тесты | ✅ | Rust: 45 (+1 генератор фикстур); Vitest: 10; Playwright: 6 сценариев — все зелёные 2026-11-08; `cargo clippy -D warnings`, `cargo fmt --check`, `tsc`, `eslint` — чисто |

## P1
| Пункт | Статус | Доказательство |
|---|---|---|
| BattleMetrics сессии (при доступе) | 🟡 | адаптер + `battlemetrics::tests::*`; API требует подписку (403 подтверждён) |
| Отряды и интервальная аналитика | ✅ | `squad_all_offline_requires_full_coverage`, `squad_overlapping_sessions_sweep`, `demo_squad_report` |
| Планировщик трекера | ✅ | `tracker_service::tests::poll_transitions_events_and_dedup`, `long_gap_breaks_session_without_events` |
| Уведомления Windows / Telegram | 🟡 | код + `notifications::tests::quiet_hours_wrap_midnight`; доставка не проверялась |
| Каталог серверов | 🟡 | battlemetrics.ru адаптер + `bmru::tests::*`; ключ не проверялся |
| История ников | ✅ | `aliases`, профиль; demo-данные |
| Экспорт/импорт | ✅ | `export::tests::export_import_roundtrip_demo`, `csv_injection_is_neutralized` |
| Фильтры, сортировка, виртуализация | ✅ | Игроки, журнал событий (`@tanstack/react-virtual`) |

## P2
| Пункт | Статус |
|---|---|
| Боевая статистика | ❌ нет источника — UI честно «не поддерживается» |
| Баны проектов | ❌ нет документированного API — слот адаптера, «не подключено» |
| Предложения тиммейтов с доказательствами | ✅ `co_sessions_require_same_server`, UI отрядов |
| Индикаторы риска (объяснимые, отключаемые) | ✅ `risk_is_explainable_and_can_be_disabled` |
| Rust+, автообновления, 24/7-хостинг | ❌ вне v1 |

## Безопасность и надёжность
| Пункт | Статус | Доказательство |
|---|---|---|
| Секреты в OS store, не в SQLite/JS/логах/экспорте | ✅/🟡 | `secrets.rs`; `redact::tests::redacts_keys`; экспорт без секретов (тест); Credential Manager — только на Windows |
| SSRF: allowlist хостов | ✅ | `http::tests::rejects_foreign_hosts`, `redact::tests::url_allowlist` |
| Строгий CSP, минимальные capabilities | 🟡 | `tauri.conf.json`, `capabilities/default.json` (валидированы tauri-build) |
| Идемпотентные вставки, без дублей событий | ✅ | `events_are_deduplicated`, `observations_idempotent_and_cascade_delete` |
| Сбой источника ≠ выход игрока | ✅ | `source_outage_is_not_logout`, трекер-тест |
| Бэкап перед миграцией, integrity check, restore | 🟡 | `db.rs`; ручная проверка не выполнялась |
| DST/часовые пояса | ✅ | `dst_transitions_berlin_spring_and_autumn` |
