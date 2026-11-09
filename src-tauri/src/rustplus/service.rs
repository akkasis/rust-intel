//! Сервис Rust+: вход Facepunch, приём спаривания, соединения с серверами, смерти тиммейтов → командный чат.

use super::client::{self, Session};
use super::deaths::{self, MemberSnap};
use super::fcm::{self, FcmCredentials, McsEvent};
use crate::infrastructure::clock::{fmt_ts, now};
use crate::persistence::repo::{self, NewEvent};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{Mutex, RwLock, mpsc};

const FCM_ACCOUNT: &str = "rustplus_fcm";
const AUTH_ACCOUNT: &str = "rustplus_auth";
const LOGIN_WINDOW: &str = "rustplus-login";
const AUTH_HOST: &str = "rid-auth.invalid";

fn token_account(id: &str) -> String {
    format!("rustplus_token_{}", id.replace([':', '.'], "_"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthData {
    steam_id: String,
    token: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct MemberView {
    pub steam_id: String,
    pub name: String,
    pub alive: bool,
    pub online: bool,
    pub grid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ServerStatus {
    pub connected: bool,
    pub via: Option<String>,
    pub map_size: Option<u32>,
    pub last_update: Option<String>,
    pub last_error: Option<String>,
    pub members: Vec<MemberView>,
    pub deaths_reported: u32,
}

#[derive(Default)]
struct Runtime {
    fcm_task: Option<JoinHandle<()>>,
    servers: HashMap<String, (JoinHandle<()>, mpsc::Sender<String>)>,
}

static RUNTIME: LazyLock<Mutex<Runtime>> = LazyLock::new(Default::default);
static STATUS: LazyLock<RwLock<HashMap<String, ServerStatus>>> = LazyLock::new(Default::default);
static LISTENER: LazyLock<RwLock<String>> = LazyLock::new(|| RwLock::new("off".into()));
static PUSH_LOG: LazyLock<RwLock<Vec<PushLogEntry>>> = LazyLock::new(Default::default);
static DIAG: LazyLock<RwLock<Diag>> = LazyLock::new(Default::default);
/// (отправлено_в, получено_в)
type SelfTest = Option<(String, Option<String>)>;
static SELFTEST: LazyLock<RwLock<SelfTest>> = LazyLock::new(Default::default);

#[derive(Debug, Clone, Serialize)]
pub struct PushLogEntry {
    pub at: String,
    pub channel: Option<String>,
    pub title: Option<String>,
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Diag {
    pub last_login_at: Option<String>,
    pub last_error: Option<String>,
    pub reconnects: u32,
    pub facepunch_registered_at: Option<String>,
    pub facepunch_error: Option<String>,
}

/// Для журнала: вырезаем значение playerToken (это секрет), остальное оставляем для диагностики.
fn redact_token(body: &str) -> String {
    let re = regex::Regex::new(r#"("playerToken"\s*:\s*)("?)[-0-9]+("?)"#).expect("regex");
    re.replace_all(body, "$1$2***$3").chars().take(1500).collect()
}

async fn push_log(channel: Option<String>, title: Option<String>, result: String) {
    let mut l = PUSH_LOG.write().await;
    l.insert(0, PushLogEntry { at: fmt_ts(now()), channel, title, result });
    l.truncate(15);
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ServerRow {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: i64,
    pub player_id: String,
    pub enabled: bool,
    pub death_alerts_chat: bool,
    pub death_alerts_local: bool,
    pub paired_at: String,
    pub last_connected_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ServerView {
    #[serde(flatten)]
    pub row: ServerRow,
    pub status: ServerStatus,
}

#[derive(Debug, Serialize)]
pub struct RustPlusStatus {
    pub device_registered: bool,
    pub steam_linked: bool,
    pub steam_id: Option<String>,
    pub listener: String,
    pub servers: Vec<ServerView>,
    pub demo: bool,
    pub push_log: Vec<PushLogEntry>,
    pub diag: Diag,
    /// (отправлено_в, получено_в)
    pub selftest: Option<(String, Option<String>)>,
}

fn creds(state: &AppState) -> Option<FcmCredentials> {
    state.secrets.get_raw(FCM_ACCOUNT).and_then(|s| serde_json::from_str(&s).ok())
}
fn auth(state: &AppState) -> Option<AuthData> {
    state.secrets.get_raw(AUTH_ACCOUNT).and_then(|s| serde_json::from_str(&s).ok())
}

async fn pool(state: &AppState) -> Option<sqlx::SqlitePool> {
    state.ctx().await.ok().map(|c| c.pool)
}

pub async fn list_servers(pool: &sqlx::SqlitePool) -> Result<Vec<ServerRow>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM rustplus_servers ORDER BY paired_at DESC").fetch_all(pool).await
}

pub async fn status(state: &AppState) -> RustPlusStatus {
    let demo = state.config().await.mode == Some(crate::config::Mode::Demo);
    let rows = match pool(state).await {
        Some(p) => list_servers(&p).await.unwrap_or_default(),
        None => vec![],
    };
    let st = STATUS.read().await;
    RustPlusStatus {
        device_registered: creds(state).is_some(),
        steam_linked: auth(state).is_some(),
        steam_id: auth(state).map(|a| a.steam_id),
        listener: LISTENER.read().await.clone(),
        servers: rows.into_iter().map(|r| ServerView { status: st.get(&r.id).cloned().unwrap_or_default(), row: r }).collect(),
        demo,
        push_log: PUSH_LOG.read().await.clone(),
        diag: DIAG.read().await.clone(),
        selftest: SELFTEST.read().await.clone(),
    }
}

/// Запуск при старте приложения: слушатель уведомлений и соединения с включёнными серверами.
pub async fn start(app: AppHandle, state: Arc<AppState>) {
    if creds(&state).is_some() && auth(&state).is_some() {
        start_listener(app.clone(), state.clone()).await;
    }
    if let Some(p) = pool(&state).await {
        for s in list_servers(&p).await.unwrap_or_default().into_iter().filter(|s| s.enabled) {
            start_server(app.clone(), state.clone(), s).await;
        }
    }
}

// ------------------------------------------------------------------ вход и регистрация

/// Шаг 1–2: регистрирует устройство (если нужно) и открывает окно входа Facepunch через Steam.
pub async fn begin_login(app: AppHandle, state: Arc<AppState>) -> Result<(), String> {
    if creds(&state).is_none() {
        let c = fcm::register_device().await.map_err(|e| format!("Регистрация устройства: {e}"))?;
        state.secrets.set_raw(FCM_ACCOUNT, &serde_json::to_string(&c).unwrap_or_default())?;
    }
    if let Some(w) = app.get_webview_window(LOGIN_WINDOW) {
        let _ = w.set_focus();
        return Ok(());
    }
    // Страница входа Rust+ передаёт результат в ReactNativeWebView.postMessage (как в мобильном приложении).
    // Перенаправляем это сообщение на служебный адрес и перехватываем навигацию — в сеть ничего не уходит.
    let init = format!(
        "(function(){{try{{if(!window.ReactNativeWebView){{window.ReactNativeWebView={{postMessage:function(m){{location.href='https://{AUTH_HOST}/cb?d='+encodeURIComponent(m);}}}};}}}}catch(e){{}}}})();"
    );
    let app2 = app.clone();
    let state2 = state.clone();
    let url: tauri::Url = "https://companion-rust.facepunch.com/login".parse().map_err(|e: url::ParseError| e.to_string())?;
    tauri::WebviewWindowBuilder::new(&app, LOGIN_WINDOW, tauri::WebviewUrl::External(url))
        .title("Rust+ — вход через Steam")
        .inner_size(520.0, 760.0)
        .initialization_script(&init)
        .on_navigation(move |u| {
            if u.host_str() != Some(AUTH_HOST) {
                return true;
            }
            let data = u.query_pairs().find(|(k, _)| k == "d").map(|(_, v)| v.to_string());
            let app3 = app2.clone();
            let st = state2.clone();
            tauri::async_runtime::spawn(async move {
                let r = finish_login(app3.clone(), st, data).await;
                if let Some(w) = app3.get_webview_window(LOGIN_WINDOW) {
                    let _ = w.close();
                }
                let _ = app3.emit("rid://rustplus", r.err());
            });
            false
        })
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn finish_login(app: AppHandle, state: Arc<AppState>, data: Option<String>) -> Result<(), String> {
    let v: serde_json::Value = serde_json::from_str(&data.ok_or("пустой ответ входа")?).map_err(|e| e.to_string())?;
    let token = v["Token"].as_str().ok_or("в ответе нет Token")?.to_string();
    let steam_id = v["SteamId"].as_str().map(str::to_string).or_else(|| v["SteamId"].as_u64().map(|x| x.to_string())).unwrap_or_default();
    let c = creds(&state).ok_or("устройство не зарегистрировано")?;
    fcm::register_with_facepunch(&token, &c.expo_token).await.map_err(|e| e.to_string())?;
    state.secrets.set_raw(AUTH_ACCOUNT, &serde_json::to_string(&AuthData { steam_id, token }).unwrap_or_default())?;
    start_listener(app, state).await;
    Ok(())
}

pub async fn logout(state: &AppState) {
    let mut rt = RUNTIME.lock().await;
    if let Some(t) = rt.fcm_task.take() {
        t.abort();
    }
    state.secrets.delete_raw(AUTH_ACCOUNT);
    state.secrets.delete_raw(FCM_ACCOUNT);
    *LISTENER.write().await = "off".into();
}

// ------------------------------------------------------------------ слушатель уведомлений

async fn start_listener(app: AppHandle, state: Arc<AppState>) {
    let mut rt = RUNTIME.lock().await;
    if let Some(t) = rt.fcm_task.take() {
        t.abort();
    }
    rt.fcm_task = Some(tauri::async_runtime::spawn(listener_loop(app, state)));
}

/// Повторная регистрация устройства в Facepunch (регистрация может истекать). Делается при каждом запуске слушателя.
pub async fn reregister(state: &AppState) -> Result<(), String> {
    let (Some(c), Some(a)) = (creds(state), auth(state)) else { return Err("сначала войдите через Steam".into()) };
    match fcm::register_with_facepunch(&a.token, &c.expo_token).await {
        Ok(()) => {
            let mut d = DIAG.write().await;
            d.facepunch_registered_at = Some(fmt_ts(now()));
            d.facepunch_error = None;
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            DIAG.write().await.facepunch_error = Some(msg.clone());
            Err(msg)
        }
    }
}

/// Самопроверка доставки: отправить себе уведомление и ждать его в потоке.
pub async fn self_test(state: &AppState) -> Result<(), String> {
    let c = creds(state).ok_or("устройство не зарегистрировано")?;
    let nonce = uuid::Uuid::new_v4().to_string();
    *SELFTEST.write().await = Some((fmt_ts(now()), None));
    SELFTEST_NONCE.write().await.replace(nonce.clone());
    fcm::send_self_test(&c.expo_token, &nonce).await.map_err(|e| e.to_string())
}

static SELFTEST_NONCE: LazyLock<RwLock<Option<String>>> = LazyLock::new(Default::default);

async fn listener_loop(app: AppHandle, state: Arc<AppState>) {
    let mut backoff = 2u64;
    if let Err(e) = reregister(&state).await {
        tracing::warn!(error = %e, "Rust+: повторная регистрация в Facepunch не удалась");
    }
    loop {
        let Some(c) = creds(&state) else { return };
        *LISTENER.write().await = "connecting".into();
        let ids: Vec<String> = match pool(&state).await {
            Some(p) => repo::get_setting(&p, "rustplus_persistent_ids").await.ok().flatten().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
            None => vec![],
        };
        let (tx, mut rx) = mpsc::unbounded_channel::<fcm::PushData>();
        let listen = fcm::listen(&c, &ids, move |e| match e {
            McsEvent::LoggedIn => {
                let _ = tx.send(fcm::PushData { persistent_id: None, data: HashMap::from([("__login".into(), "1".into())]) });
            }
            McsEvent::Data(d) => {
                let _ = tx.send(d);
            }
        });
        let handler = async {
            while let Some(d) = rx.recv().await {
                if d.data.contains_key("__login") {
                    *LISTENER.write().await = "connected".into();
                    DIAG.write().await.last_login_at = Some(fmt_ts(now()));
                    let _ = app.emit("rid://rustplus", None::<String>);
                    continue;
                }
                if let Some(pid) = &d.persistent_id
                    && let Some(p) = pool(&state).await
                {
                    let mut ids: Vec<String> =
                        repo::get_setting(&p, "rustplus_persistent_ids").await.ok().flatten().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
                    ids.push(pid.clone());
                    let n = ids.len();
                    if n > 50 {
                        ids.drain(..n - 50);
                    }
                    let _ = repo::set_setting(&p, "rustplus_persistent_ids", &serde_json::to_string(&ids).unwrap_or_default()).await;
                }
                let channel = d.data.get("channelId").cloned();
                let title = d.data.get("title").cloned();
                if let Some(nonce) = d.data.get("rid_selftest").cloned().or_else(|| {
                    d.data
                        .get("body")
                        .and_then(|b| serde_json::from_str::<serde_json::Value>(b).ok())
                        .and_then(|v| v["rid_selftest"].as_str().map(str::to_string))
                }) {
                    let ok = SELFTEST_NONCE.read().await.as_deref() == Some(nonce.as_str());
                    if ok && let Some(st) = SELFTEST.write().await.as_mut() {
                        st.1 = Some(fmt_ts(now()));
                    }
                    push_log(channel, title, "самопроверка: доставка работает".into()).await;
                    let _ = app.emit("rid://rustplus", None::<String>);
                    continue;
                }
                let result = match fcm::parse_pairing_detailed(&d) {
                    Ok(pair) if pair.kind == "server" => match on_paired(app.clone(), state.clone(), pair.clone()).await {
                        Ok(()) => format!("сервер «{}» добавлен", pair.name),
                        Err(e) => {
                            tracing::warn!(error = %e, "Rust+: не удалось сохранить спаривание");
                            format!("ошибка сохранения сервера: {e}")
                        }
                    },
                    Ok(pair) => format!("спаривание устройства ({}) — не поддерживается, нужен «Pair with server»", pair.kind),
                    Err(reason) if reason != "не спаривание" => {
                        let body = d.data.get("body").map(|b| redact_token(b)).unwrap_or_default();
                        format!("не удалось разобрать спаривание: {reason}. body: {body}")
                    }
                    Err(_) => "другое уведомление (не спаривание)".into(),
                };
                push_log(channel, title, result).await;
                let _ = app.emit("rid://rustplus", None::<String>);
            }
        };
        tokio::select! {
            r = listen => {
                if let Err(e) = r {
                    tracing::warn!(error = %e, "Rust+: поток уведомлений прерван");
                    let mut d = DIAG.write().await;
                    d.last_error = Some(e.to_string());
                    d.reconnects += 1;
                }
            }
            _ = handler => {}
        }
        *LISTENER.write().await = "reconnecting".into();
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(120);
    }
}

async fn on_paired(app: AppHandle, state: Arc<AppState>, p: fcm::Pairing) -> Result<(), String> {
    if !client::valid_target(&p.ip, p.port) {
        return Err("недопустимый адрес сервера".into());
    }
    let id = format!("{}:{}", p.ip, p.port);
    state.secrets.set_raw(&token_account(&id), &p.player_token.to_string())?;
    let pool = pool(&state).await.ok_or("база недоступна")?;
    sqlx::query(
        "INSERT INTO rustplus_servers(id, name, ip, port, player_id, enabled, death_alerts_chat, death_alerts_local, paired_at)
         VALUES (?, ?, ?, ?, ?, 1, 1, 1, ?)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, player_id = excluded.player_id, enabled = 1, paired_at = excluded.paired_at, last_error = NULL",
    )
    .bind(&id)
    .bind(&p.name)
    .bind(&p.ip)
    .bind(p.port as i64)
    .bind(p.player_id.to_string())
    .bind(fmt_ts(now()))
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;
    let row: ServerRow = sqlx::query_as("SELECT * FROM rustplus_servers WHERE id = ?").bind(&id).fetch_one(&pool).await.map_err(|e| e.to_string())?;
    start_server(app.clone(), state, row).await;
    let _ = app.emit("rid://rustplus", None::<String>);
    Ok(())
}

// ------------------------------------------------------------------ соединения с серверами

pub async fn start_server(app: AppHandle, state: Arc<AppState>, row: ServerRow) {
    let mut rt = RUNTIME.lock().await;
    if let Some((t, _)) = rt.servers.remove(&row.id) {
        t.abort();
    }
    let (tx, rx) = mpsc::channel::<String>(16);
    let id = row.id.clone();
    rt.servers.insert(id, (tauri::async_runtime::spawn(server_loop(app, state, row, rx)), tx));
}

pub async fn stop_server(id: &str) {
    if let Some((t, _)) = RUNTIME.lock().await.servers.remove(id) {
        t.abort();
    }
    let mut st = STATUS.write().await;
    if let Some(s) = st.get_mut(id) {
        s.connected = false;
    }
}

pub async fn send_chat(id: &str, text: &str) -> Result<(), String> {
    let rt = RUNTIME.lock().await;
    let (_, tx) = rt.servers.get(id).ok_or("сервер не подключён")?;
    tx.send(text.to_string()).await.map_err(|e| e.to_string())
}

async fn set_status<F: FnOnce(&mut ServerStatus)>(id: &str, f: F) {
    let mut st = STATUS.write().await;
    f(st.entry(id.to_string()).or_default());
}

async fn server_loop(app: AppHandle, state: Arc<AppState>, row: ServerRow, mut outbox: mpsc::Receiver<String>) {
    let mut backoff = 5u64;
    let id = row.id.clone();
    loop {
        let token: Option<i32> = state.secrets.get_raw(&token_account(&id)).and_then(|t| t.parse().ok());
        let player_id: Option<u64> = row.player_id.parse().ok();
        let (Some(token), Some(player_id)) = (token, player_id) else {
            set_status(&id, |s| s.last_error = Some("нет токена — спарьте сервер заново в игре".into())).await;
            return;
        };
        let err = match client::connect(&row.ip, row.port as u16).await {
            Ok((ws, via)) => {
                backoff = 5;
                set_status(&id, |s| {
                    s.connected = true;
                    s.via = Some(via.into());
                    s.last_error = None;
                })
                .await;
                if let Some(p) = pool(&state).await {
                    let _ = sqlx::query("UPDATE rustplus_servers SET last_connected_at = ?, last_error = NULL WHERE id = ?")
                        .bind(fmt_ts(now()))
                        .bind(&id)
                        .execute(&p)
                        .await;
                }
                let _ = app.emit("rid://rustplus", None::<String>);
                run_session(&app, &state, &row, Session::new(ws, player_id, token), &mut outbox).await
            }
            Err(e) => e,
        };
        tracing::warn!(server = %id, error = %err, "Rust+: соединение потеряно");
        set_status(&id, |s| {
            s.connected = false;
            s.last_error = Some(err.clone());
        })
        .await;
        if let Some(p) = pool(&state).await {
            let _ = sqlx::query("UPDATE rustplus_servers SET last_error = ? WHERE id = ?").bind(&err).bind(&id).execute(&p).await;
        }
        let _ = app.emit("rid://rustplus", None::<String>);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(120);
    }
}

async fn run_session(app: &AppHandle, state: &Arc<AppState>, row: &ServerRow, mut s: Session, outbox: &mut mpsc::Receiver<String>) -> String {
    let id = row.id.clone();
    if let Err(e) = s.get_info().await {
        return e;
    }
    let mut map_size: Option<u32> = None;
    let mut prev: Option<HashMap<u64, MemberSnap>> = None;
    let mut tick = tokio::time::interval(Duration::from_secs(4));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if let Err(e) = s.get_team_info().await { return e; }
            }
            Some(text) = outbox.recv() => {
                if let Err(e) = s.send_team_message(&text).await { return e; }
            }
            msg = s.next() => {
                let m = match msg {
                    None => return "сервер закрыл соединение".into(),
                    Some(Err(e)) => return e,
                    Some(Ok(m)) => m,
                };
                let team = m.response.as_ref().and_then(|r| r.team_info.clone())
                    .or_else(|| m.broadcast.as_ref().and_then(|b| b.team_changed.as_ref()).and_then(|t| t.team_info.clone()));
                if let Some(info) = m.response.as_ref().and_then(|r| r.info.as_ref()) {
                    map_size = info.map_size;
                    set_status(&id, |st| st.map_size = map_size).await;
                }
                if let Some(e) = m.response.as_ref().and_then(|r| r.error.as_ref()) {
                    let text = e.error.clone().unwrap_or_default();
                    set_status(&id, |st| st.last_error = Some(format!("Rust+: {text}"))).await;
                    if text == "not_found" || text == "invalid_token" || text == "banned" {
                        return format!("сервер отклонил токен ({text}) — спарьте сервер заново");
                    }
                }
                if let Some(t) = team {
                    let cur = deaths::snapshot(&t.members);
                    let died = deaths::detect(prev.as_ref(), &cur, map_size);
                    let members: Vec<MemberView> = t.members.iter().map(|mm| MemberView {
                        steam_id: mm.steam_id.unwrap_or(0).to_string(),
                        name: mm.name.clone().unwrap_or_default(),
                        alive: mm.is_alive.unwrap_or(true),
                        online: mm.is_online.unwrap_or(false),
                        grid: map_size.and_then(|ms| super::grid::grid_pos(mm.x.unwrap_or(-1.0) as f64, mm.y.unwrap_or(-1.0) as f64, ms as f64)),
                    }).collect();
                    let n = died.len() as u32;
                    set_status(&id, |st| { st.members = members; st.last_update = Some(fmt_ts(now())); st.deaths_reported += n; }).await;
                    for d in died {
                        handle_death(app, state, row, &mut s, &d).await;
                    }
                    if n > 0 { let _ = app.emit("rid://rustplus", None::<String>); }
                    prev = Some(cur);
                }
            }
        }
    }
}

async fn handle_death(app: &AppHandle, state: &Arc<AppState>, row: &ServerRow, s: &mut Session, d: &deaths::Death) {
    let Ok(ctx) = state.ctx().await else { return };
    let tz = ctx.tz();
    let local = now().with_timezone(&tz).format("%H:%M").to_string();
    let text = deaths::chat_text(d, &local);
    if row.death_alerts_chat
        && let Err(e) = s.send_team_message(&text).await
    {
        tracing::warn!(error = %e, "Rust+: не удалось отправить в чат");
    }
    let steam = d.steam_id.to_string();
    let known = repo::get_player(&ctx.pool, &steam).await.ok().flatten().is_some();
    let ev = repo::insert_event(
        &ctx.pool,
        &NewEvent {
            steam_id: known.then_some(steam.as_str()),
            provider: "rustplus",
            kind: "teammate_died",
            dedup_key: format!("rustplus:death:{}:{}:{}", row.id, d.steam_id, d.death_time),
            occurred_lower: now(),
            occurred_upper: now(),
            approximate: false,
            server_id: None,
            details: serde_json::json!({"name": d.name, "grid": d.grid, "server": row.name, "rustplus": true}),
        },
    )
    .await;
    if row.death_alerts_local
        && let Ok(Some(id)) = ev
    {
        crate::commands::dispatch_events(app, ctx, vec![id]).await;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn token_redacted_in_log() {
        let b = super::redact_token(r#"{"ip":"1.2.3.4","playerToken":"-12345","port":1}"#);
        assert!(b.contains("\"playerToken\":\"***\""), "{b}");
        assert!(!super::redact_token(r#"{"playerToken":987654}"#).contains("987654"));
    }

    #[test]
    fn token_account_is_safe() {
        assert_eq!(super::token_account("203.0.113.5:28082"), "rustplus_token_203_0_113_5_28082");
    }
}
