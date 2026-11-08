//! Генерация фикстур для браузерного mock-режима UI и e2e-тестов из РЕАЛЬНОГО демо-сида.
//! Запуск: `cargo test --lib export_ui_fixtures -- --ignored`

#[cfg(test)]
mod tests {
    use crate::commands::{build_bans_overview, build_dashboard, build_integrations};
    use crate::persistence::repo;
    use crate::providers::demo::{PLAYERS, demo_steam_id};
    use crate::services::player_service::tests::demo_ctx;
    use crate::services::{RangeSpec, analytics_service, player_service, server_service};
    use serde_json::{Map, Value, json};

    #[tokio::test]
    #[ignore]
    async fn export_ui_fixtures() {
        let ctx = demo_ctx().await;
        let mut m = Map::new();
        let v = |x: &dyn erased::Ser| x.to_value();
        m.insert("get_dashboard".into(), v(&build_dashboard(&ctx).await.unwrap()));
        m.insert("list_players".into(), v(&repo::list_players(&ctx.pool, None, false, 500, 0).await.unwrap()));
        m.insert("list_squads".into(), v(&repo::list_squads(&ctx.pool).await.unwrap()));
        m.insert("list_tracked".into(), v(&repo::list_tracked(&ctx.pool).await.unwrap()));
        m.insert("list_events".into(), v(&repo::list_events(&ctx.pool, None, None, false, 300, 0).await.unwrap()));
        m.insert("get_bans_overview".into(), v(&build_bans_overview(&ctx).await.unwrap()));
        m.insert("list_servers".into(), v(&repo::list_servers(&ctx.pool, None, false, 500).await.unwrap()));
        m.insert("get_integrations".into(), v(&build_integrations(&ctx).await.unwrap()));
        let mut profiles = Map::new();
        let mut activity = Map::new();
        for p in PLAYERS {
            let id = demo_steam_id(p.n).to_string();
            profiles.insert(id.clone(), v(&player_service::get_profile(&ctx, &id).await.unwrap()));
            for preset in ["7d", "30d", "90d"] {
                let mut r = analytics_service::activity(&ctx, &id, &RangeSpec { preset: preset.into(), from: None, to: None }, Some("Europe/Moscow"), None)
                    .await
                    .unwrap();
                r.sessions.truncate(60);
                activity.insert(format!("{id}:{preset}"), v(&r));
            }
        }
        m.insert("get_player_profile".into(), Value::Object(profiles));
        m.insert("get_activity".into(), Value::Object(activity));
        let mut squads = Map::new();
        for s in repo::list_squads(&ctx.pool).await.unwrap() {
            for preset in ["7d", "30d"] {
                let r = analytics_service::squad_report(&ctx, s.id, &RangeSpec { preset: preset.into(), from: None, to: None }, Some("Europe/Moscow"))
                    .await
                    .unwrap();
                squads.insert(format!("{}:{preset}", s.id), v(&r));
            }
        }
        m.insert("get_squad_report".into(), Value::Object(squads));
        let mut pop = Map::new();
        for s in repo::list_servers(&ctx.pool, None, false, 50).await.unwrap() {
            pop.insert(s.id.clone(), v(&server_service::population(&ctx, &s.id, "7d").await.unwrap()));
        }
        m.insert("get_server_population".into(), Value::Object(pop));
        // BattleMetrics
        use crate::persistence::bm_repo;
        use crate::providers::demo::demo_bm_id;
        use crate::services::bm_service;
        m.insert("list_bm_players".into(), v(&bm_repo::list_players(&ctx.pool, None, false, 300).await.unwrap()));
        m.insert("list_server_watch".into(), v(&bm_repo::list_watch(&ctx.pool).await.unwrap()));
        let mut bmp = Map::new();
        let mut bma = Map::new();
        let mut ids: Vec<String> = PLAYERS.iter().map(|p| demo_bm_id(p.n)).collect();
        ids.extend(["9200001".to_string(), "9200002".to_string(), "9300001".to_string()]);
        for id in &ids {
            let mut view = bm_service::player_view(&ctx, id, false).await.unwrap();
            view.sessions.truncate(80);
            bmp.insert(id.clone(), v(&view));
            for preset in ["7d", "30d", "90d"] {
                bma.insert(
                    format!("{id}:{preset}"),
                    v(&bm_service::activity(&ctx, id, &RangeSpec { preset: preset.into(), from: None, to: None }, Some("Europe/Moscow")).await.unwrap()),
                );
            }
        }
        m.insert("get_bm_player".into(), Value::Object(bmp));
        m.insert("get_bm_activity".into(), Value::Object(bma));
        let mut online = Map::new();
        for s in repo::list_servers(&ctx.pool, None, false, 50).await.unwrap() {
            online.insert(s.id.clone(), v(&bm_service::server_online(&ctx, &s.id).await.unwrap()));
        }
        m.insert("get_server_online".into(), Value::Object(online));
        m.insert("_meta".into(), json!({"generated_from": "providers::demo::seed", "note": "ВЫМЫШЛЕННЫЕ данные демо-режима"}));
        let path = format!("{}/../src/lib/api/mock-data.json", env!("CARGO_MANIFEST_DIR"));
        std::fs::write(&path, serde_json::to_string(&Value::Object(m)).unwrap()).unwrap();
    }

    mod erased {
        pub trait Ser {
            fn to_value(&self) -> serde_json::Value;
        }
        impl<T: serde::Serialize> Ser for T {
            fn to_value(&self) -> serde_json::Value {
                serde_json::to_value(self).unwrap()
            }
        }
    }
}
