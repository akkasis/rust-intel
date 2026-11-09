//! WebSocket-клиент Rust+: прямое подключение ws://ip:port, при неудаче — официальный прокси Facepunch.

use super::proto::*;
use futures_util::{SinkExt, StreamExt};
use prost::Message as _;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMsg;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Разрешаем только публичные адреса/имена (защита от обращения к локальной сети через подставное уведомление).
pub fn valid_target(ip: &str, port: u16) -> bool {
    if port == 0 || ip.is_empty() || ip.len() > 253 {
        return false;
    }
    if let Ok(addr) = ip.parse::<std::net::Ipv4Addr>() {
        return !(addr.is_loopback() || addr.is_link_local() || addr.is_multicast() || addr.is_unspecified() || addr.is_broadcast());
    }
    ip.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-') && ip.contains('.') && !ip.eq_ignore_ascii_case("localhost")
}

fn tls_connector() -> Connector {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("tls versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Connector::Rustls(Arc::new(cfg))
}

/// base64url без выравнивания (RFC 4648 §5).
pub fn base64url(s: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s.as_bytes())
}

/// Подключение через сокет Facepunch по GUID сервера: токен входа Rust+ передаётся
/// как WebSocket-подпротокол «rustplus.auth.v1, <base64url(token)>» (как в RustPlusApi / Rust+ Desktop).
pub async fn connect_companion(server_id: &str, auth_token: &str) -> Result<Ws, String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    if !super::fcm::is_guid(server_id) {
        return Err("некорректный ID сервера".into());
    }
    let mut req = format!("wss://companion-rust.facepunch.com/socket/{server_id}").into_client_request().map_err(|e| e.to_string())?;
    let proto = format!("rustplus.auth.v1, {}", base64url(auth_token));
    req.headers_mut().insert("Sec-WebSocket-Protocol", proto.parse().map_err(|_| "некорректный токен входа".to_string())?);
    match tokio::time::timeout(Duration::from_secs(15), tokio_tungstenite::connect_async_tls_with_config(req, None, false, Some(tls_connector()))).await {
        Ok(Ok((ws, _))) => Ok(ws),
        Ok(Err(tokio_tungstenite::tungstenite::Error::Http(r))) if matches!(r.status().as_u16(), 401 | 403) => {
            Err(format!("Facepunch отклонил вход Rust+ (HTTP {}) — войдите через Steam заново: «Отключить Rust+» → «Подключить Rust+»", r.status().as_u16()))
        }
        Ok(Err(e)) => Err(format!("сокет Facepunch: {e}")),
        Err(_) => Err("таймаут подключения к сокету Facepunch".into()),
    }
}

/// Подключение: сначала сокет Facepunch (если есть GUID сервера), затем прямое ws://ip:port для старых спариваний.
/// Старый прокси /game/{ip}/{port} не используется — Facepunch отвечает на него 418.
pub async fn connect(server_id: Option<&str>, auth_token: Option<&str>, ip: &str, port: u16) -> Result<(Ws, &'static str), String> {
    let mut last = String::from("нет маршрута подключения");
    if let (Some(id), Some(tok)) = (server_id, auth_token) {
        match connect_companion(id, tok).await {
            Ok(ws) => return Ok((ws, "facepunch_socket")),
            Err(e) => last = e,
        }
    }
    if valid_target(ip, port) {
        let direct = format!("ws://{ip}:{port}");
        match tokio::time::timeout(Duration::from_secs(10), tokio_tungstenite::connect_async(direct.as_str())).await {
            Ok(Ok((ws, _))) => return Ok((ws, "direct")),
            Ok(Err(e)) => last = format!("{last}; прямое: {e}"),
            Err(_) => last = format!("{last}; прямое: таймаут"),
        }
    }
    Err(last)
}

pub struct Session {
    pub ws: Ws,
    pub player_id: u64,
    pub player_token: i32,
    seq: u32,
}

impl Session {
    pub fn new(ws: Ws, player_id: u64, player_token: i32) -> Self {
        Self { ws, player_id, player_token, seq: 0 }
    }

    fn base(&mut self) -> AppRequest {
        self.seq = self.seq.wrapping_add(1);
        AppRequest { seq: self.seq, player_id: self.player_id, player_token: self.player_token, ..Default::default() }
    }

    async fn send(&mut self, r: AppRequest) -> Result<(), String> {
        self.ws.send(WsMsg::Binary(r.encode_to_vec().into())).await.map_err(|e| e.to_string())
    }

    pub async fn get_info(&mut self) -> Result<(), String> {
        let mut r = self.base();
        r.get_info = Some(AppEmpty {});
        self.send(r).await
    }

    pub async fn get_team_info(&mut self) -> Result<(), String> {
        let mut r = self.base();
        r.get_team_info = Some(AppEmpty {});
        self.send(r).await
    }

    pub async fn send_team_message(&mut self, text: &str) -> Result<(), String> {
        let mut r = self.base();
        r.send_team_message = Some(AppSendMessage { message: text.chars().take(128).collect() });
        self.send(r).await
    }

    /// Следующее сообщение от сервера (None — соединение закрыто).
    pub async fn next(&mut self) -> Option<Result<AppMessage, String>> {
        loop {
            match self.ws.next().await? {
                Ok(WsMsg::Binary(b)) => return Some(AppMessage::decode(b.as_ref()).map_err(|e| e.to_string())),
                Ok(WsMsg::Ping(p)) => {
                    let _ = self.ws.send(WsMsg::Pong(p)).await;
                }
                Ok(WsMsg::Close(_)) => return None,
                Ok(_) => {}
                Err(e) => return Some(Err(e.to_string())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_validation() {
        assert!(valid_target("203.0.113.5", 28082));
        assert!(valid_target("rust.example.com", 28082));
        assert!(!valid_target("127.0.0.1", 28082));
        assert!(!valid_target("localhost", 28082));
        assert!(!valid_target("169.254.169.254", 80));
        assert!(!valid_target("203.0.113.5", 0));
        assert!(!valid_target("evil/../x", 1));
    }
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    use crate::rustplus::deaths;

    /// Фальшивый сервер Rust+: отвечает на getTeamInfo (сначала все живы, потом один погиб) и принимает сообщение в чат.
    #[tokio::test]
    async fn fake_server_death_to_team_chat() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let mut n = 0;
            let mut chat = None;
            while let Some(Ok(m)) = ws.next().await {
                let WsMsg::Binary(b) = m else { continue };
                let req = AppRequest::decode(b.as_ref()).unwrap();
                assert_eq!(req.player_token, -42);
                if let Some(t) = req.send_team_message {
                    chat = Some(t.message);
                    break;
                }
                if req.get_team_info.is_some() {
                    n += 1;
                    let alive = n == 1;
                    let msg = AppMessage {
                        response: Some(AppResponse {
                            seq: Some(req.seq),
                            team_info: Some(AppTeamInfo {
                                leader_steam_id: Some(1),
                                members: vec![Member {
                                    steam_id: Some(76561197960287930),
                                    name: Some("Клещ".into()),
                                    x: Some(2000.0),
                                    y: Some(2000.0),
                                    is_online: Some(true),
                                    spawn_time: Some(1),
                                    is_alive: Some(alive),
                                    death_time: Some(if alive { 0 } else { 1234 }),
                                }],
                            }),
                            ..Default::default()
                        }),
                        broadcast: None,
                    };
                    ws.send(WsMsg::Binary(msg.encode_to_vec().into())).await.unwrap();
                }
            }
            chat
        });
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}")).await.unwrap();
        let mut s = Session::new(ws, 76561197960287930, -42);
        let mut prev = None;
        for _ in 0..2 {
            s.get_team_info().await.unwrap();
            let m = s.next().await.unwrap().unwrap();
            let cur = deaths::snapshot(&m.response.unwrap().team_info.unwrap().members);
            for d in deaths::detect(prev.as_ref(), &cur, Some(4500)) {
                s.send_team_message(&deaths::chat_text(&d, "21:05")).await.unwrap();
            }
            prev = Some(cur);
        }
        let chat = server.await.unwrap();
        assert_eq!(chat.as_deref(), Some("☠ Клещ умер в N16 (21:05)"));
    }
}

#[cfg(test)]
mod live_socket {
    /// Проверка маршрута Facepunch /socket/{serverId}: только рукопожатие и getInfo с пустым токеном.
    /// cargo test --lib live_socket_route -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_socket_route() {
        let none = super::connect_companion("2ae66e35-ffee-436d-bd57-f3a107386f7a", "").await;
        println!("без токена: {:?}", none.err());
        let r = super::connect_companion("2ae66e35-ffee-436d-bd57-f3a107386f7a", "fake-token").await;
        println!("с поддельным токеном: {:?}", r.err());
    }
}
