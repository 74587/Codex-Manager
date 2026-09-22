use super::*;
use axum::{routing::any, Json, Router};
use codexmanager_core::storage::{now_ts, Account};
use std::ffi::OsString;
use std::time::Duration;

struct Restore(Vec<(&'static str, Option<OsString>)>);
impl Restore {
    fn set(&mut self, key: &'static str, value: Option<&str>) {
        self.0.push((key, std::env::var_os(key)));
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}
impl Drop for Restore {
    fn drop(&mut self) {
        for (key, value) in self.0.drain(..).rev() {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        crate::usage_http::reload_usage_http_client_from_env();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_redemption_cannot_start_another_charge() {
    let _env = crate::test_env_guard();
    let dir = std::env::temp_dir().join(format!("reset-async-{}", random_uuid_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut restore = Restore(Vec::new());
    restore.set("CODEXMANAGER_DATABASE_URL", None);
    restore.set("CODEXMANAGER_STORAGE_BACKEND", Some("sqlite"));
    restore.set(
        "CODEXMANAGER_DB_PATH",
        Some(dir.join("fixture.db").to_str().unwrap()),
    );
    restore.set("CODEXMANAGER_UPSTREAM_PROXY_URL", Some(""));
    restore.set("NO_PROXY", Some("*"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    restore.set(
        "CODEXMANAGER_USAGE_BASE_URL",
        Some(&format!("http://{}", listener.local_addr().unwrap())),
    );
    crate::usage_http::reload_usage_http_client_from_env();
    crate::storage_helpers::initialize_storage().unwrap();
    let storage = open_storage().unwrap();
    let id = format!("reset-cancel-{}", random_uuid_v4());
    storage
        .insert_account(&Account {
            id: id.clone(),
            label: "cancel fixture".into(),
            issuer: "fixture".into(),
            chatgpt_account_id: None,
            workspace_id: None,
            group_name: None,
            sort: 0,
            status: "active".into(),
            created_at: now_ts(),
            updated_at: now_ts(),
        })
        .unwrap();
    storage
        .insert_token(&Token {
            account_id: id.clone(),
            id_token: String::new(),
            access_token: "fixture-access".into(),
            refresh_token: String::new(),
            api_key_access_token: None,
            last_refresh: now_ts(),
        })
        .unwrap();
    drop(storage);
    let posted = Arc::new(tokio::sync::Notify::new());
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let app = Router::new().fallback(any({
        let posted = posted.clone();
        let count = count.clone();
        move |method: axum::http::Method| {
            let posted = posted.clone();
            let count = count.clone();
            async move {
                if method == axum::http::Method::POST {
                    count.fetch_add(1, Ordering::SeqCst);
                    posted.notify_one();
                    std::future::pending::<()>().await;
                }
                Json(serde_json::json!({"available_count":1}))
            }
        }
    }));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let work = tokio::spawn({
        let id = id.clone();
        async move { consume_reset_credit_async(&id).await }
    });
    tokio::time::timeout(Duration::from_secs(5), posted.notified())
        .await
        .expect("provider sees redemption");
    work.abort();
    assert!(work.await.unwrap_err().is_cancelled());
    let error = consume_reset_credit_async(&id).await.unwrap_err();
    assert_eq!(error, RESET_CREDIT_LOCK_POISONED_MESSAGE);
    assert_eq!(count.load(Ordering::SeqCst), 1);
    server.abort();
    let _ = server.await;
}
