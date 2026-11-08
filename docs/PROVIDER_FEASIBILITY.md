# Проверка источников данных (Provider feasibility)

Дата проверки: **2026-11-08**. Окружение: macOS (разработка), без пользовательских ключей.
Статусы: `verified` — подтверждено реальным запросом; `documented` — подтверждено документацией, без запроса;
`denied` — источник ответил отказом; `not_tested` — нет ключа/подписки, запрос не выполнялся; `unsupported` — источник не даёт таких данных.

## 1. Steam Web API (официальный) — основной источник

| Возможность | Endpoint | Ключ | Статус | Поля → модель |
|---|---|---|---|---|
| Разрешение vanity | `GET /ISteamUser/ResolveVanityURL/v1/?vanityurl=&url_type=1` | Steam Web API key | documented, not_tested | `response.steamid`, `success` (1 ok, 42 нет совпадения) |
| Профиль | `GET /ISteamUser/GetPlayerSummaries/v2/?steamids=` (≤100) | да | documented, not_tested | `personaname`, `avatarfull`, `profileurl`, `communityvisibilitystate` (3 public / 1 private), `timecreated` (только публичные), `loccountrycode`, `personastate`, `gameid` |
| «В Rust сейчас» | то же, `gameid == "252490"` | да | documented, not_tested | Presence `online/offline/unknown`, **без сервера** |
| Баны | `GET /ISteamUser/GetPlayerBans/v1/?steamids=` | да | documented, not_tested | `VACBanned`, `NumberOfVACBans`, `NumberOfGameBans`, `DaysSinceLastBan`, `CommunityBanned`, `EconomyBan` |
| Часы в Rust | `GET /IPlayerService/GetOwnedGames/v1/?steamid=&include_played_free_games=1&appids_filter[0]=252490` | да | documented, not_tested | `games[].playtime_forever` (мин); пустой `response` → скрыто |

Контрактные тесты: `providers::steam::tests::*` на санитизированных фикстурах `tests/fixtures/steam/*.json`.
Ограничения: лимит 100 000 вызовов/сутки на ключ; Game Ban не указывает игру; закрытый профиль → presence `unknown`;
невидимый режим Steam и скрытые «игровые данные» дают ложное «не в Rust» — это ограничение источника (см. KNOWN_LIMITATIONS).

## 2. BattleMetrics.com API (официальный, платный доступ)

**Проверено запросом 2026-11-08 без токена:**
```
GET https://api.battlemetrics.com/servers?filter[game]=rust&page[size]=1  → 403
{"errors":[{"status":"403","title":"Forbidden","detail":"Access denied. A subscription is required to use the API."}]}
GET https://api.battlemetrics.com/players?filter[search]=test             → 403 (то же сообщение)
```
Страница документации `https://www.battlemetrics.com/developers/documentation` вернула 403 автоматическому клиенту, поэтому
пути ниже взяты из ранее опубликованной документации JSON:API и **не перепроверены в этой сессии**. Статус: `denied` без подписки, `not_tested` с токеном.

| Возможность | Endpoint | Статус |
|---|---|---|
| Сопоставление SteamID → player id | `POST /players/match` body `{data:[{type:"identifier",attributes:{type:"steamID",identifier}}]}` | not_tested; привязка только при точном совпадении `steamID` |
| Поиск по нику | `GET /players?filter[search]=&filter[server][game]=rust` | not_tested; кандидаты без SteamID, без автопривязки |
| Сессии игрока | `GET /players/{id}/relationships/sessions?include=server` | not_tested; `attributes.start/stop`, `relationships.server` |
| Каталог серверов | `GET /servers?filter[game]=rust&filter[search]=` | not_tested |
| История онлайна сервера | `GET /servers/{id}/player-count-history?start&stop&resolution=60` | not_tested |

Адаптер реализован (`providers/battlemetrics.rs`), без имитации успеха: при 401/403 — `forbidden`, статус «доступ запрещён» в UI.
Контрактные тесты на санитизированных фикстурах: `tests/fixtures/battlemetrics/`.

## 3. battlemetrics.ru API v1 (официальный, бесплатный ключ) — только серверный уровень

Документация: `https://battlemetrics.ru/api` (прочитана 2026-11-08). Авторизация `Authorization: Bearer bm_live_…`, 60 запросов/мин/ключ,
заголовки `X-RateLimit-*`, 429 + `Retry-After`. Данные обновляются раз в минуту.

| Возможность | Endpoint | Статус |
|---|---|---|
| Каталог | `GET /api/v1/servers?game=rust&sort=players&per_page=50&q=` | documented, not_tested |
| Сервер | `GET /api/v1/servers/{id}` | documented, not_tested; `players = null`, если `trust != confirmed` — хранится как null, не 0 |
| История онлайна | `GET /api/v1/servers/{id}/history?period=24h|7d|30d` | documented, not_tested; шаги без опросов пропущены, а не нули |
| Вайпы | `GET /api/v1/wipes?game=rust&when=upcoming|past` | documented, not_tested; `fact=true` — наблюдаемый вайп. Корневой ключ массива в документации не указан — парсер принимает массив или `wipes/items/data` |
| Игроки | — | **unsupported**: нет поиска игроков и истории игроков (только голоса по нику для владельца сервера) |

**Не путать с battlemetrics.com.** История онлайна сервера никогда не интерпретируется как сессии игрока.

## 4. Баны проектов

Подтверждённых публичных документированных API банов Rust-проектов не найдено → `unsupported`. Слот адаптера
описан в `providers::descriptors()`. Произвольные URL пользователем не принимаются (SSRF-защита: allowlist хостов в `infrastructure/http.rs`).

## 5. Telegram Bot API

`POST https://api.telegram.org/bot{token}/sendMessage` — документирован, `not_tested` (нет токена). Токен редактируется в логах.

## 6. Не используются (сознательно)

RustRecon и иные закрытые/аккаунтные ресурсы, скрейпинг, обход лимитов/CAPTCHA/подписок, Rust+ (требует отдельной валидации, P2).
