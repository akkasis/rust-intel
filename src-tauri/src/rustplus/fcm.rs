//! Регистрация «устройства» Rust+ для push-уведомлений (как телефон с приложением Rust+) и приём
//! уведомлений по протоколу MCS (mtalk.google.com:5228). Порт liamcottle/push-receiver (AndroidFCM).
//! Конфигурация Firebase — публичная конфигурация приложения Rust+ Companion (используется rustplus.js).

use super::proto::*;
use prost::Message;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const API_KEY: &str = "AIzaSyB5y2y-Tzqb4-I4Qnlsh_9naYv_TD8pCvY";
const PROJECT_ID: &str = "rust-companion-app";
const GCM_SENDER_ID: &str = "976529667804";
const GMS_APP_ID: &str = "1:976529667804:android:d6f1ddeb4403b338fea619";
const ANDROID_PACKAGE: &str = "com.facepunch.rust.companion";
const ANDROID_CERT: &str = "E28D05345FB78A7A1A63D70F4A302DBF426CA5AD";
const EXPO_PROJECT_ID: &str = "49451aca-a822-41e6-ad59-955718d0ff9c";
const FIREBASE_CLIENT: &str = "android-min-sdk/23 fire-core/20.0.0 device-name/a21snnxx device-brand/samsung device-model/a21s android-installer/com.android.vending fire-android/30 fire-installations/17.0.0 fire-fcm/22.0.0 android-platform/ kotlin/1.9.23 android-target-sdk/34";

pub const MCS_VERSION: u8 = 41;
pub const TAG_HEARTBEAT_PING: u8 = 0;
pub const TAG_HEARTBEAT_ACK: u8 = 1;
pub const TAG_LOGIN_REQUEST: u8 = 2;
pub const TAG_LOGIN_RESPONSE: u8 = 3;
pub const TAG_CLOSE: u8 = 4;
pub const TAG_DATA_MESSAGE: u8 = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FcmCredentials {
    pub android_id: String,
    pub security_token: String,
    pub fcm_token: String,
    pub expo_token: String,
}

#[derive(Debug, thiserror::Error)]
pub enum FcmError {
    #[error("HTTP: {0}")]
    Http(String),
    #[error("Ответ: {0}")]
    Bad(String),
    #[error("Сеть: {0}")]
    Io(#[from] std::io::Error),
    #[error("Протокол: {0}")]
    Decode(#[from] prost::DecodeError),
}

fn http() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(20)).redirect(reqwest::redirect::Policy::none()).build().expect("client")
}

fn h(e: reqwest::Error) -> FcmError {
    FcmError::Http(crate::infrastructure::redact::redact(&e.to_string()))
}

/// Android checkin → (android_id, security_token). Новая регистрация.
pub async fn checkin() -> Result<(u64, u64), FcmError> {
    checkin_with(None).await
}

/// Повторный checkin уже выданного устройства — обязателен перед каждым подключением к MCS
/// (так делает push-receiver; без него вход проходит, но сообщения могут не доставляться).
pub async fn checkin_existing(android_id: &str, security_token: &str) -> Result<(u64, u64), FcmError> {
    let id: u64 = android_id.parse().map_err(|_| FcmError::Bad("android_id".into()))?;
    let tok: u64 = security_token.parse().map_err(|_| FcmError::Bad("security_token".into()))?;
    checkin_with(Some((id, tok))).await
}

async fn checkin_with(existing: Option<(u64, u64)>) -> Result<(u64, u64), FcmError> {
    let req = AndroidCheckinRequest {
        id: existing.map(|e| e.0 as i64),
        checkin: AndroidCheckinProto {
            r#type: Some(3), // DEVICE_CHROME_BROWSER
            chrome_build: Some(ChromeBuildProto { platform: Some(2), chrome_version: Some("63.0.3234.0".into()), channel: Some(1) }),
        },
        security_token: existing.map(|e| e.1),
        version: Some(3),
        user_serial_number: Some(0),
    };
    let body = http()
        .post("https://android.clients.google.com/checkin")
        .header("Content-Type", "application/x-protobuf")
        .body(req.encode_to_vec())
        .send()
        .await
        .map_err(h)?
        .error_for_status()
        .map_err(h)?
        .bytes()
        .await
        .map_err(h)?;
    let r = AndroidCheckinResponse::decode(body.as_ref())?;
    match (r.android_id, r.security_token) {
        (Some(a), Some(s)) if a != 0 => Ok((a, s)),
        _ => Err(FcmError::Bad("checkin без android_id".into())),
    }
}

fn firebase_fid() -> String {
    use base64::Engine;
    let mut b: [u8; 17] = rand::random();
    b[0] = 0b0111_0000 | (b[0] & 0b0000_1111);
    base64::engine::general_purpose::STANDARD_NO_PAD.encode(b)
}

async fn firebase_install() -> Result<String, FcmError> {
    let v: serde_json::Value = http()
        .post(format!("https://firebaseinstallations.googleapis.com/v1/projects/{PROJECT_ID}/installations"))
        .header("X-Android-Package", ANDROID_PACKAGE)
        .header("X-Android-Cert", ANDROID_CERT)
        .header("x-firebase-client", FIREBASE_CLIENT)
        .header("x-firebase-client-log-type", "3")
        .header("x-goog-api-key", API_KEY)
        .header("User-Agent", "Dalvik/2.1.0 (Linux; U; Android 11; SM-A217F Build/RP1A.200720.012)")
        .json(&serde_json::json!({"fid": firebase_fid(), "appId": GMS_APP_ID, "authVersion": "FIS_v2", "sdkVersion": "a:17.0.0"}))
        .send()
        .await
        .map_err(h)?
        .json()
        .await
        .map_err(h)?;
    v["authToken"]["token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| FcmError::Bad(format!("Firebase installation: {}", v.to_string().chars().take(200).collect::<String>())))
}

async fn register3(android_id: u64, security_token: u64, install_token: &str) -> Result<String, FcmError> {
    let form = [
        ("device", android_id.to_string()),
        ("app", ANDROID_PACKAGE.into()),
        ("cert", ANDROID_CERT.into()),
        ("app_ver", "1".into()),
        ("X-subtype", GCM_SENDER_ID.into()),
        ("X-app_ver", "1".into()),
        ("X-osv", "29".into()),
        ("X-cliv", "fiid-21.1.1".into()),
        ("X-gmsv", "220217001".into()),
        ("X-scope", "*".into()),
        ("X-Goog-Firebase-Installations-Auth", install_token.into()),
        ("X-gms_app_id", GMS_APP_ID.into()),
        ("X-Firebase-Client", FIREBASE_CLIENT.into()),
        ("X-Firebase-Client-Log-Type", "1".into()),
        ("X-app_ver_name", "1".into()),
        ("target_ver", "31".into()),
        ("sender", GCM_SENDER_ID.into()),
    ];
    let body = serde_urlencoded_form(&form);
    for attempt in 0..5 {
        let text = http()
            .post("https://android.clients.google.com/c2dm/register3")
            .header("Authorization", format!("AidLogin {android_id}:{security_token}"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body.clone())
            .send()
            .await
            .map_err(h)?
            .text()
            .await
            .map_err(h)?;
        if !text.contains("Error") {
            if let Some(token) = text.split_once('=').map(|x| x.1.trim().to_string()).filter(|t| !t.is_empty()) {
                return Ok(token);
            }
        }
        tracing::warn!(attempt, "register3: {}", text.chars().take(80).collect::<String>());
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(FcmError::Bad("GCM register3 не удался".into()))
}

fn serde_urlencoded_form(pairs: &[(&str, String)]) -> String {
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        s.append_pair(k, v);
    }
    s.finish()
}

async fn expo_token(fcm_token: &str) -> Result<String, FcmError> {
    let v: serde_json::Value = http()
        .post("https://exp.host/--/api/v2/push/getExpoPushToken")
        .json(&serde_json::json!({
            "type": "fcm",
            "deviceId": uuid::Uuid::new_v4().to_string(),
            "development": false,
            "appId": ANDROID_PACKAGE,
            "deviceToken": fcm_token,
            "projectId": EXPO_PROJECT_ID,
        }))
        .send()
        .await
        .map_err(h)?
        .json()
        .await
        .map_err(h)?;
    v["data"]["expoPushToken"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| FcmError::Bad(format!("Expo: {}", v.to_string().chars().take(200).collect::<String>())))
}

/// Полная регистрация устройства (без входа Steam).
pub async fn register_device() -> Result<FcmCredentials, FcmError> {
    let install = firebase_install().await?;
    let (android_id, security_token) = checkin().await?;
    let fcm_token = register3(android_id, security_token, &install).await?;
    let expo = expo_token(&fcm_token).await?;
    Ok(FcmCredentials { android_id: android_id.to_string(), security_token: security_token.to_string(), fcm_token, expo_token: expo })
}

/// Тестовое уведомление самому себе через Expo — тем же маршрутом, которым Facepunch шлёт спаривание.
pub async fn send_self_test(expo_token: &str, nonce: &str) -> Result<(), FcmError> {
    let v: serde_json::Value = http()
        .post("https://exp.host/--/api/v2/push/send")
        .json(&serde_json::json!({"to": expo_token, "title": "RID", "body": "self-test", "channelId": "rid-selftest", "data": {"rid_selftest": nonce}}))
        .send()
        .await
        .map_err(h)?
        .json()
        .await
        .map_err(h)?;
    if v["data"]["status"].as_str() == Some("ok") {
        Ok(())
    } else {
        Err(FcmError::Bad(format!("Expo: {}", v.to_string().chars().take(200).collect::<String>())))
    }
}

/// Регистрирует устройство в Facepunch для аккаунта Steam (AuthToken со страницы входа Rust+).
pub async fn register_with_facepunch(auth_token: &str, expo_token: &str) -> Result<(), FcmError> {
    let r = http()
        .post("https://companion-rust.facepunch.com/api/push/register")
        .json(&serde_json::json!({"AuthToken": auth_token, "DeviceId": "rid-desktop", "PushKind": 3, "PushToken": expo_token}))
        .send()
        .await
        .map_err(h)?;
    if r.status().is_success() { Ok(()) } else { Err(FcmError::Bad(format!("Facepunch push/register: HTTP {}", r.status().as_u16()))) }
}

// ------------------------------------------------------------------ MCS

fn varint(mut v: usize, out: &mut Vec<u8>) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

pub fn login_packet(android_id: &str, security_token: &str, persistent_ids: &[String]) -> Vec<u8> {
    let hex = android_id.parse::<u64>().map(|v| format!("{v:x}")).unwrap_or_default();
    let req = LoginRequest {
        id: "chrome-63.0.3234.0".into(),
        domain: "mcs.android.com".into(),
        user: android_id.into(),
        resource: android_id.into(),
        auth_token: security_token.into(),
        device_id: Some(format!("android-{hex}")),
        setting: vec![Setting { name: "new_vc".into(), value: "1".into() }],
        received_persistent_id: persistent_ids.to_vec(),
        adaptive_heartbeat: Some(false),
        use_rmq2: Some(true),
        auth_service: Some(2),
        network_type: Some(1),
    };
    let body = req.encode_to_vec();
    let mut out = vec![MCS_VERSION, TAG_LOGIN_REQUEST];
    varint(body.len(), &mut out);
    out.extend(body);
    out
}

pub fn packet(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    varint(body.len(), &mut out);
    out.extend_from_slice(body);
    out
}

/// Входящее уведомление (данные приложения: ключ → значение).
#[derive(Debug, Clone)]
pub struct PushData {
    pub persistent_id: Option<String>,
    pub data: std::collections::HashMap<String, String>,
}

async fn read_varint<R: AsyncReadExt + Unpin>(r: &mut R) -> Result<usize, FcmError> {
    let mut shift = 0;
    let mut v: usize = 0;
    loop {
        let b = r.read_u8().await?;
        v |= ((b & 0x7f) as usize) << shift;
        if b & 0x80 == 0 {
            return Ok(v);
        }
        shift += 7;
        if shift > 28 {
            return Err(FcmError::Bad("длинный varint".into()));
        }
    }
}

pub enum McsEvent {
    LoggedIn,
    Data(PushData),
}

/// Подключается к MCS и вызывает `on_event` на вход/уведомления. Возвращается при разрыве.
pub async fn listen<F>(creds: &FcmCredentials, persistent_ids: &[String], mut on_event: F) -> Result<(), FcmError>
where
    F: FnMut(McsEvent) + Send,
{
    checkin_existing(&creds.android_id, &creds.security_token).await?;
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| FcmError::Bad(e.to_string()))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(cfg));
    let tcp = tokio::time::timeout(Duration::from_secs(15), tokio::net::TcpStream::connect(("mtalk.google.com", 5228)))
        .await
        .map_err(|_| FcmError::Bad("таймаут подключения".into()))??;
    let name = rustls::pki_types::ServerName::try_from("mtalk.google.com").map_err(|e| FcmError::Bad(e.to_string()))?;
    let mut tls = connector.connect(name, tcp).await?;
    tls.write_all(&login_packet(&creds.android_id, &creds.security_token, persistent_ids)).await?;
    tls.flush().await?;
    let version = tls.read_u8().await?;
    if version < 38 {
        return Err(FcmError::Bad(format!("версия MCS {version}")));
    }
    loop {
        // сервер шлёт heartbeat-пинги; если долго тишина — переподключаемся
        let tag = match tokio::time::timeout(Duration::from_secs(60 * 20), tls.read_u8()).await {
            Ok(t) => t?,
            Err(_) => return Err(FcmError::Bad("нет данных 20 минут".into())),
        };
        let size = read_varint(&mut tls).await?;
        if size > 1 << 20 {
            return Err(FcmError::Bad("слишком большое сообщение".into()));
        }
        let mut buf = vec![0u8; size];
        tls.read_exact(&mut buf).await?;
        match tag {
            TAG_LOGIN_RESPONSE => {
                let r = LoginResponse::decode(buf.as_slice())?;
                if let Some(e) = r.error.filter(|e| e.code.unwrap_or(0) != 0) {
                    return Err(FcmError::Bad(format!("вход MCS: {:?}", e.message)));
                }
                on_event(McsEvent::LoggedIn);
            }
            TAG_HEARTBEAT_PING => {
                tls.write_all(&packet(TAG_HEARTBEAT_ACK, &HeartbeatAck::default().encode_to_vec())).await?;
            }
            TAG_CLOSE => return Err(FcmError::Bad("сервер закрыл соединение".into())),
            TAG_DATA_MESSAGE => {
                let m = DataMessageStanza::decode(buf.as_slice())?;
                on_event(McsEvent::Data(PushData { persistent_id: m.persistent_id, data: m.app_data.into_iter().map(|a| (a.key, a.value)).collect() }));
            }
            _ => {}
        }
    }
}

/// Данные спаривания из уведомления Rust+ (channelId = pairing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pairing {
    pub kind: String,
    pub name: String,
    /// GUID сервера в Facepunch (новые спаривания) — основной способ подключения.
    pub server_id: Option<String>,
    /// IP/порт — только у старых спариваний.
    pub ip: Option<String>,
    pub port: Option<u16>,
    pub player_id: u64,
    pub player_token: i32,
}

pub fn parse_pairing(p: &PushData) -> Option<Pairing> {
    parse_pairing_detailed(p).ok()
}

/// Экранирует сырые управляющие символы внутри JSON-строк (серверные описания иногда содержат
/// настоящие переводы строк/табы — строгий JSON их не допускает).
fn escape_raw_controls(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_str = false;
    let mut esc = false;
    for c in s.chars() {
        if in_str {
            if esc {
                esc = false;
                out.push(c);
                continue;
            }
            match c {
                '\\' => {
                    esc = true;
                    out.push(c);
                }
                '"' => {
                    in_str = false;
                    out.push(c);
                }
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push(' '),
                c => out.push(c),
            }
        } else {
            if c == '"' {
                in_str = true;
            }
            out.push(c);
        }
    }
    out
}

/// Значение поля как текст: строка или число (в т.ч. дробное/большое — без потери точности для целых).
fn field_text(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.trim().to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Разбор с точной причиной отказа (для журнала).
pub fn parse_pairing_detailed(p: &PushData) -> Result<Pairing, String> {
    let raw = p.data.get("body").ok_or("нет поля body")?;
    let body: serde_json::Value =
        serde_json::from_str(raw).or_else(|_| serde_json::from_str(&escape_raw_controls(raw))).map_err(|e| format!("body не JSON: {e}"))?;
    // иногда данные вложены ещё одним слоем ("data" / строка JSON)
    let body = match (&body["ip"], &body["data"]) {
        (serde_json::Value::Null, serde_json::Value::Object(_)) => body["data"].clone(),
        (serde_json::Value::Null, serde_json::Value::String(s)) => serde_json::from_str(s).unwrap_or(body),
        _ => body,
    };
    let is_pairing = p.data.get("channelId").map(String::as_str) == Some("pairing")
        || p.data.get("gcm.notification.android_channel_id").map(String::as_str) == Some("pairing")
        || body.get("playerToken").is_some();
    if !is_pairing {
        return Err("не спаривание".into());
    }
    let server_id = field_text(&body["id"]).filter(|s| is_guid(s));
    let ip = field_text(&body["ip"]).filter(|s| !s.is_empty());
    let port = match field_text(&body["port"]) {
        Some(port_s) => Some(port_s.parse::<f64>().ok().filter(|v| *v >= 1.0 && *v <= 65535.0).map(|v| v as u16).ok_or(format!("плохой port «{port_s}»"))?),
        None => None,
    };
    if server_id.is_none() && (ip.is_none() || port.is_none()) {
        return Err("нет ни ID сервера, ни ip/port".into());
    }
    let pid_s = field_text(&body["playerId"]).ok_or("нет playerId")?;
    let player_id: u64 = pid_s.parse().map_err(|_| format!("плохой playerId «{pid_s}»"))?;
    let tok_s = field_text(&body["playerToken"]).ok_or("нет playerToken")?;
    // playerToken — int32 в протоколе; встречается и как беззнаковое 32-битное число
    let player_token: i32 = tok_s
        .parse::<i64>()
        .ok()
        .and_then(|v| {
            if (i32::MIN as i64..=i32::MAX as i64).contains(&v) {
                Some(v as i32)
            } else if (0..=u32::MAX as i64).contains(&v) {
                Some(v as u32 as i32)
            } else {
                None
            }
        })
        .ok_or("плохой playerToken")?;
    Ok(Pairing {
        kind: field_text(&body["type"]).unwrap_or_else(|| "server".into()),
        name: crate::infrastructure::redact::sanitize_text(body["name"].as_str().unwrap_or("Сервер"), 120),
        server_id,
        ip,
        port,
        player_id,
        player_token,
    })
}

pub fn is_guid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5 && [8, 4, 4, 4, 12].iter().zip(&parts).all(|(n, p)| p.len() == *n && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_packet_shape() {
        let p = login_packet("1234567890", "987654321", &["0:1%abc".into()]);
        assert_eq!(p[0], MCS_VERSION);
        assert_eq!(p[1], TAG_LOGIN_REQUEST);
        // varint длины + тело декодируется обратно
        let len = p[2] as usize; // короткое тело < 128? иначе 2 байта
        let (len, off) = if len & 0x80 == 0 { (len, 3) } else { ((len & 0x7f) | ((p[3] as usize) << 7), 4) };
        let r = LoginRequest::decode(&p[off..off + len]).unwrap();
        assert_eq!(r.device_id.as_deref(), Some("android-499602d2"));
        assert_eq!(r.received_persistent_id, vec!["0:1%abc".to_string()]);
    }

    #[test]
    fn pairing_rustoria_like_variants() {
        let mk = |body: &str| {
            let mut data = std::collections::HashMap::new();
            data.insert("channelId".into(), "pairing".into());
            data.insert("body".into(), body.to_string());
            PushData { persistent_id: None, data }
        };
        // реальные переводы строк в описании + числа вместо строк + токен как беззнаковое
        let raw = "{\"id\":\"2ae66e35\",\"name\":\"Rustoria.co - EU East Medium\",\"desc\":\"Wipe Schedule:\n\tMap - Bi-Weekly\",\"img\":\"\",\"logo\":\"\",\"url\":\"https://rustoria.co\",\"ip\":\"185.206.151.10\",\"port\":28083,\"type\":\"server\",\"playerId\":\"76561197960287930\",\"playerToken\":3000000000}";
        let p = parse_pairing_detailed(&mk(raw)).unwrap();
        assert_eq!(p.ip.as_deref(), Some("185.206.151.10"));
        assert_eq!(p.port, Some(28083));
        assert_eq!(p.player_token, 3000000000u32 as i32);
        // экранированные \\n и строки
        let esc = r#"{"name":"S","desc":"a\\n\\tb","ip":"1.2.3.4","port":"28082","type":"server","playerId":"76561197960287930","playerToken":"-5"}"#;
        assert_eq!(parse_pairing_detailed(&mk(esc)).unwrap().player_token, -5);
        // понятная причина при отсутствии поля
        let bad = r#"{"name":"S","ip":"1.2.3.4","type":"server","playerId":"76561197960287930","playerToken":"1"}"#;
        assert_eq!(parse_pairing_detailed(&mk(bad)).unwrap_err(), "нет ни ID сервера, ни ip/port");
        // новый формат Facepunch: только GUID сервера (как у Rustoria)
        let newfmt = r#"{"id":"2ae66e35-ffee-436d-bd57-f3a107386f7a","name":"Rustoria.co - EU East Medium","desc":"Wipe Schedule:\n\tMap","img":"","logo":"","url":"http://rustoria.co","playerId":"76561198932658360","playerToken":"-987654","type":"server"}"#;
        let p = parse_pairing_detailed(&mk(newfmt)).unwrap();
        assert_eq!(p.server_id.as_deref(), Some("2ae66e35-ffee-436d-bd57-f3a107386f7a"));
        assert!(p.ip.is_none() && p.port.is_none());
    }

    #[test]
    fn pairing_notification_parsed() {
        let mut data = std::collections::HashMap::new();
        data.insert("channelId".into(), "pairing".into());
        data.insert(
            "body".into(),
            r#"{"id":"x","name":"Test Server","desc":"","img":"","logo":"","url":"","ip":"203.0.113.5","port":"28082","type":"server","playerId":"76561197960287930","playerToken":"-1234567"}"#.into(),
        );
        let p = parse_pairing(&PushData { persistent_id: None, data }).unwrap();
        assert_eq!(p.port, Some(28082));
        assert_eq!(p.player_token, -1234567);
        assert_eq!(p.player_id, 76561197960287930);
        let mut other = std::collections::HashMap::new();
        other.insert("channelId".into(), "alarm".into());
        assert!(parse_pairing(&PushData { persistent_id: None, data: other }).is_none());
    }

    /// Живая проверка доставки: регистрация → MCS → push самому себе через Expo (маршрут Facepunch).
    /// Запуск: cargo test --lib live_push_roundtrip -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_push_roundtrip() {
        let creds = register_device().await.expect("регистрация");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let c2 = creds.clone();
        let task = tokio::spawn(async move {
            let _ = listen(&c2, &[], move |e| {
                let _ = tx.send(match e {
                    McsEvent::LoggedIn => None,
                    McsEvent::Data(d) => Some(d),
                });
            })
            .await;
        });
        // ждём вход
        let first = tokio::time::timeout(Duration::from_secs(20), rx.recv()).await.expect("вход MCS");
        assert!(first.unwrap().is_none());
        let body = serde_json::json!({"ip":"203.0.113.5","port":"28082","name":"Test","type":"server","playerId":"76561197960287930","playerToken":"-1"});
        let r: serde_json::Value = http()
            .post("https://exp.host/--/api/v2/push/send")
            .json(&serde_json::json!({"to": creds.expo_token, "title": "Test", "body": "pair", "channelId": "pairing", "data": body}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        println!("expo send: {r}");
        match tokio::time::timeout(Duration::from_secs(40), rx.recv()).await {
            Ok(Some(Some(d))) => {
                println!("ПОЛУЧЕНО: {:?}", d.data.keys().collect::<Vec<_>>());
                for (k, v) in &d.data {
                    println!("  {k} = {}", v.chars().take(200).collect::<String>());
                }
                println!("parse_pairing: {:?}", parse_pairing(&d));
            }
            other => println!("НЕ ПОЛУЧЕНО: {other:?}"),
        }
        task.abort();
    }

    /// Живая проверка регистрации устройства и входа в MCS (сеть). Запуск: cargo test --lib live_fcm -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_fcm_register_and_login() {
        let creds = register_device().await.expect("регистрация");
        println!("android_id ok, fcm token len {}, expo {}", creds.fcm_token.len(), &creds.expo_token[..20.min(creds.expo_token.len())]);
        let mut logged = false;
        let r = tokio::time::timeout(
            Duration::from_secs(20),
            listen(&creds, &[], |e| {
                if let McsEvent::LoggedIn = e {
                    logged = true;
                    println!("MCS: вход выполнен");
                }
            }),
        )
        .await;
        println!("listen result (timeout ожидаем): {}", r.is_err());
        assert!(logged || r.is_err());
    }
}
