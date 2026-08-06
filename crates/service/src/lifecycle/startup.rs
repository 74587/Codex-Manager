use std::io;
use std::thread;

pub struct ServerHandle {
    pub addr: String,
    join: thread::JoinHandle<()>,
}

impl ServerHandle {
    /// 函数 `join`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    ///
    /// # 返回
    /// 无
    pub fn join(self) {
        let _ = self.join.join();
    }
}

/// 函数 `start_one_shot_server`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn start_one_shot_server() -> std::io::Result<ServerHandle> {
    crate::portable::bootstrap_current_process();
    crate::gateway::reload_runtime_config_from_env();
    crate::storage_helpers::initialize_storage()
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
    crate::sync_runtime_settings_from_storage();
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
    let addr = server
        .server_addr()
        .to_ip()
        .map(|a| a.to_string())
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "server addr missing"))?;
    let join = thread::spawn(move || {
        if let Some(request) = server.incoming_requests().next() {
            crate::http::backend_router::handle_backend_request(request);
        }
    });
    Ok(ServerHandle { addr, join })
}

/// 函数 `start_server`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
///
/// # 返回
/// 返回函数执行结果
pub fn start_server(addr: &str) -> std::io::Result<()> {
    crate::portable::bootstrap_current_process();
    crate::gateway::reload_runtime_config_from_env();
    crate::storage_helpers::initialize_storage()
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
    crate::sync_runtime_settings_from_storage();
    sync_active_gateway_profile_config_on_startup();
    crate::app_settings::ensure_codex_latest_version_sync();
    crate::usage_refresh::ensure_usage_polling();
    crate::usage_refresh::ensure_gateway_keepalive();
    crate::usage_refresh::ensure_token_refresh_polling();
    crate::usage_refresh::ensure_warmup_cron();
    crate::plugin::ensure_plugin_scheduler();
    crate::http::server::start_http(addr)
}

pub(crate) fn sync_active_gateway_profile_config_on_startup() -> bool {
    let Some(storage) = crate::storage_helpers::open_storage() else {
        log::warn!(
            "event=sync_active_gateway_profile_on_startup_skipped reason=storage_unavailable"
        );
        return false;
    };
    match crate::codex_profile::sync_active_gateway_profile_config_from_storage(&storage) {
        Ok(synced) => {
            if synced {
                log::info!("event=sync_active_gateway_profile_on_startup_succeeded");
                if let Some(profile_dir) = crate::codex_profile::active_gateway_profile_dir() {
                    let config_path = profile_dir.join("config.toml");
                    let reload = crate::codex_runtime::reload_stale_codex_app_servers(
                        &profile_dir,
                        &config_path,
                    );
                    log::info!(
                        "event=reload_stale_codex_app_servers_on_startup matched_process_count={} signaled_process_count={} message={}",
                        reload.matched_process_count,
                        reload.signaled_process_count,
                        reload.message
                    );
                    for warning in reload.warnings {
                        log::warn!(
                            "event=reload_stale_codex_app_servers_on_startup_warning warning={warning}"
                        );
                    }
                }
            }
            synced
        }
        Err(err) => {
            log::warn!("event=sync_active_gateway_profile_on_startup_failed error={err}");
            false
        }
    }
}
