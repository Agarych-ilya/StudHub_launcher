#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod auth;
mod bundled;
mod drive;
mod game;
mod github_updates;
mod net;
mod pack;
mod status;
mod updater;
use anyhow::{bail, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Deployment {
    pub api_url: String,
    pub manifest_url: String,
    pub trusted_origins: Vec<String>,
    pub manifest_public_key: String,
    pub server_address: String,
    pub launcher_update_url: String,
    pub launcher_public_key: String,
    pub spawn_status_url: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub game_dir: String,
    pub java_path: String,
    pub ram_mb: u32,
    pub nickname: String,
    pub auto_connect: bool,
    pub auto_update: bool,
    pub deployment: Deployment,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            game_dir: String::new(),
            java_path: String::new(),
            ram_mb: 0,
            nickname: String::new(),
            auto_connect: true,
            auto_update: true,
            deployment: Deployment {
                server_address: "148.253.208.254:25589".into(),
                ..Default::default()
            },
        }
    }
}
#[derive(Clone, Serialize)]
pub struct Progress {
    pub stage: String,
    pub done: u64,
    pub total: u64,
    pub speed: f64,
    pub eta: f64,
}
impl Progress {
    fn steps(stage: &str, done: u64, total: u64) -> Self {
        Self {
            stage: stage.into(),
            done,
            total,
            speed: 0.0,
            eta: 0.0,
        }
    }
}
#[derive(Clone)]
pub struct Runtime {
    settings: Arc<Mutex<Settings>>,
    config_dir: PathBuf,
    pub pid: Arc<AtomicU32>,
    pub busy: Arc<AtomicBool>,
    task: Arc<tokio::sync::Mutex<()>>,
    logs: Arc<Mutex<Vec<String>>>,
    pub game_lock: Arc<Mutex<Option<fs::File>>>,
    server: Arc<Mutex<Value>>,
    news: Arc<Mutex<Value>>,
    update: Arc<Mutex<Option<Value>>>,
    github_update: Arc<Mutex<Value>>,
    drive: Arc<Mutex<Value>>,
}
impl Runtime {
    pub fn log(&self, app: &AppHandle, text: &str) {
        let text = text.chars().take(4000).collect::<String>();
        let mut logs = self.logs.lock().unwrap();
        logs.push(text.clone());
        if logs.len() > 1000 {
            logs.remove(0);
        }
        let p = self.config_dir.join("logs/launcher.log");
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::metadata(&p).map_or(false, |m| m.len() > 8 * 1024 * 1024) {
            let _ = fs::rename(&p, p.with_extension("previous.log"));
        }
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{text}");
        }
        let _ = app.emit("launcher-log", text);
    }
    pub fn progress(&self, app: &AppHandle, p: Progress) {
        let _ = app.emit("launcher-progress", p);
    }
    pub fn changed(&self, app: &AppHandle) {
        let _ = app.emit("launcher-changed", ());
    }
    fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
}
fn validate_settings(s: &Settings) -> Result<()> {
    if s.ram_mb != 0 && !(2048..=32768).contains(&s.ram_mb) {
        bail!("RAM должна быть от 2048 до 32768 МБ")
    }
    if !s.nickname.is_empty() {
        auth::offline(&s.nickname)?;
    }
    if !Path::new(&s.game_dir).is_absolute() {
        bail!("Выберите абсолютный путь установки")
    }
    for url in [
        &s.deployment.api_url,
        &s.deployment.manifest_url,
        &s.deployment.launcher_update_url,
        &s.deployment.spawn_status_url,
    ] {
        if !url.is_empty() {
            net::origin(url)?;
        }
    }
    for url in &s.deployment.trusted_origins {
        net::origin(url)?;
    }
    Ok(())
}
fn acquire_game(rt: &Runtime, s: &Settings) -> Result<fs::File> {
    let meta = Path::new(&s.game_dir).join(".studhub");
    pack::reject_link(&meta)?;
    fs::create_dir_all(&meta)?;
    let p = meta.join("game.lock");
    pack::reject_link(&p)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(p)?;
    file.try_lock_exclusive()
        .map_err(|_| anyhow::anyhow!("Игра или другой экземпляр лаунчера использует эту сборку"))?;
    if rt.pid.load(Ordering::SeqCst) != 0 {
        bail!("Minecraft уже работает")
    }
    Ok(file)
}
#[tauri::command]
fn snapshot(rt: State<'_, Runtime>) -> Result<Value, String> {
    let s = rt.settings();
    let root = Path::new(&s.game_dir);
    let own = pack::state(root).map_err(|e| e.to_string())?;
    let mods = pack::mods(root).map_err(|e| e.to_string())?;
    Ok(
        json!({"settings":s,"pid":rt.pid.load(Ordering::SeqCst),"busy":rt.busy.load(Ordering::SeqCst),
        "minecraft":auth::offline(&s.nickname).ok().map(|a|json!({"name":a.name,"id":a.id})),
        "effective_ram_mb":game::memory_limit(s.ram_mb),
        "drive":*rt.drive.lock().unwrap(),"server":*rt.server.lock().unwrap(),"news":*rt.news.lock().unwrap(),"update":*rt.update.lock().unwrap(),"github_update":*rt.github_update.lock().unwrap(),
        "pack_version":own.version,"neoforge":own.neoforge,"mods":mods,"logs":*rt.logs.lock().unwrap(),"app_version":env!("CARGO_PKG_VERSION")}),
    )
}
#[tauri::command]
fn save_settings(app: AppHandle, rt: State<'_, Runtime>, settings: Settings) -> Result<(), String> {
    if rt.busy.load(Ordering::SeqCst) || rt.pid.load(Ordering::SeqCst) != 0 {
        return Err("Дождитесь завершения задачи или игры".into());
    }
    validate_settings(&settings).map_err(|e| e.to_string())?;
    net::atomic(
        &rt.config_dir.join("settings.json"),
        &serde_json::to_vec_pretty(&settings).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    *rt.settings.lock().unwrap() = settings;
    rt.changed(&app);
    Ok(())
}
#[tauri::command]
fn open_folder(rt: State<'_, Runtime>, kind: String) -> Result<(), String> {
    let s = rt.settings();
    let p = match kind.as_str() {
        "mods" => Path::new(&s.game_dir).join("mods"),
        "game" => PathBuf::from(&s.game_dir),
        "logs" => rt.config_dir.join("logs"),
        _ => return Err("Неизвестная папка".into()),
    };
    pack::reject_link(&p).map_err(|e| e.to_string())?;
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    open::that_detached(p).map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
fn import_deployment(app: AppHandle, rt: State<'_, Runtime>, file: String) -> Result<(), String> {
    if rt.busy.load(Ordering::SeqCst) || rt.pid.load(Ordering::SeqCst) != 0 {
        return Err("Дождитесь завершения задачи или игры".into());
    }
    let bytes = fs::read(file).map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("Файл подключения слишком большой".into());
    }
    let d: Deployment = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let mut s = rt.settings();
    s.deployment = d;
    validate_settings(&s).map_err(|e| e.to_string())?;
    net::atomic(
        &rt.config_dir.join("settings.json"),
        &serde_json::to_vec_pretty(&s).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    *rt.settings.lock().unwrap() = s;
    rt.changed(&app);
    Ok(())
}
async fn remote(app: &AppHandle, rt: &Runtime, s: &Settings) {
    let (_, news, _) = tokio::join!(
        refresh_server(app, rt, s),
        auth::public_api(s, "/news"),
        drive::check(app, rt)
    );
    *rt.news.lock().unwrap() = match news {
        Ok(v) => json!({"items":v}),
        Err(e) => json!({"items":[],"error":e.to_string()}),
    };
    rt.changed(app);
}
async fn refresh_server(app: &AppHandle, rt: &Runtime, s: &Settings) {
    let mut value = match status::ping(&s.deployment.server_address).await {
        Ok(v) => v,
        Err(e) => json!({"online":false,"error":e.to_string()}),
    };
    if value["online"] == true && !s.deployment.spawn_status_url.is_empty() {
        match status::spawn(
            &s.deployment.spawn_status_url,
            &s.deployment.trusted_origins,
        )
        .await
        {
            Ok(count) => value["spawn_players"] = json!(count),
            Err(e) => value["spawn_error"] = json!(e.to_string()),
        }
    }
    value["checked_at"] = json!(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs());
    *rt.server.lock().unwrap() = value;
    rt.changed(app);
}
async fn check_github_update(app: &AppHandle, rt: &Runtime) -> Result<Value> {
    let result = github_updates::check(env!("CARGO_PKG_VERSION")).await;
    let info = match result {
        Ok(value) => value,
        Err(error) => {
            rt.log(app, &format!("Проверка версии через GitHub: {error}"));
            json!({"status":"error", "error":error.to_string()})
        }
    };
    *rt.github_update.lock().unwrap() = info.clone();
    rt.changed(app);
    Ok(info)
}
async fn automatic_update(app: &AppHandle, rt: &Runtime) {
    let s = rt.settings();
    if s.deployment.launcher_update_url.is_empty() || rt.pid.load(Ordering::SeqCst) != 0 {
        return;
    }
    let Ok(_task) = rt.task.try_lock() else {
        return;
    };
    let Ok(_game) = acquire_game(rt, &s) else {
        return;
    };
    rt.busy.store(true, Ordering::SeqCst);
    rt.changed(app);
    let _busy = BusyGuard {
        rt: rt.clone(),
        app: app.clone(),
    };
    if let Err(e) = app_update(app, rt, &s, true).await {
        rt.log(app, &format!("Автообновление: {e}"));
    }
}
async fn app_update(app: &AppHandle, rt: &Runtime, s: &Settings, download: bool) -> Result<Value> {
    let d = &s.deployment;
    if d.launcher_update_url.is_empty() {
        bail!("Источник обновлений приложения не подключён")
    }
    let c = net::client()?;
    let v = net::signed(
        net::json(&c, &d.launcher_update_url, &d.trusted_origins).await?,
        &d.launcher_public_key,
    )?;
    let version = v["version"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Нет версии приложения"))?;
    if semver::Version::parse(version)? <= semver::Version::parse(env!("CARGO_PKG_VERSION"))? {
        *rt.update.lock().unwrap() = None;
        return Ok(json!({"current":true}));
    }
    let platform = if cfg!(windows) {
        "windows-amd64"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "darwin-arm64"
        } else {
            "darwin-x86_64"
        }
    } else {
        if cfg!(target_arch = "aarch64") {
            "linux-aarch64"
        } else {
            "linux-x86_64"
        }
    };
    let item = &v["artifacts"][platform];
    net::trusted(
        item["url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Нет сборки для платформы"))?,
        &d.trusted_origins,
    )
    .await?;
    let info = json!({"version":version,"artifact":item});
    *rt.update.lock().unwrap() = Some(info.clone());
    rt.changed(app);
    if download {
        let url = item["url"].as_str().unwrap();
        let parsed = url::Url::parse(url)?;
        let name = parsed
            .path_segments()
            .and_then(|mut v| v.next_back())
            .unwrap_or("");
        if !regex::Regex::new(r"^[A-Za-z0-9_.-]+$")?.is_match(name) || name == "." || name == ".." {
            bail!("Неверное имя установщика")
        }
        updater::installer_name(name, item["installer"].as_str().unwrap_or(""))?;
        let dest = rt.config_dir.join("updates").join(name);
        let transfer_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(1200))
            .build()?;
        net::download(
            &transfer_client,
            url,
            &d.trusted_origins,
            &dest,
            item["size"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("Нет размера"))?,
            item["sha256"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Нет SHA-256"))?,
            false,
            |n| {
                rt.progress(
                    app,
                    Progress::steps(
                        "Загрузка обновления приложения",
                        n,
                        item["size"].as_u64().unwrap(),
                    ),
                )
            },
        )
        .await?;
        updater::handoff(&dest)?;
        rt.log(
            app,
            "Обновление проверено. Лаунчер закроется, установится новая версия и запустится снова.",
        );
        app.exit(0);
    }
    Ok(info)
}
struct BusyGuard {
    rt: Runtime,
    app: AppHandle,
}
impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.rt.busy.store(false, Ordering::SeqCst);
        self.rt.changed(&self.app);
    }
}
#[tauri::command]
async fn action(app: AppHandle, rt: State<'_, Runtime>, kind: String) -> Result<Value, String> {
    let _task = rt
        .task
        .try_lock()
        .map_err(|_| "Другая задача ещё выполняется".to_string())?;
    rt.busy.store(true, Ordering::SeqCst);
    rt.changed(&app);
    let _busy = BusyGuard {
        rt: rt.inner().clone(),
        app: app.clone(),
    };
    let s = rt.settings();
    let result:Result<Value>=async {
        match kind.as_str() {
            "refresh"=>{remote(&app,&rt,&s).await;Ok(json!({}))},
            "sync"=>{let _lock=acquire_game(&rt,&s)?;let m=pack::sync(&app,&rt,&s).await?;Ok(json!({"version":m.version}))},
            "install"=>{let _lock=acquire_game(&rt,&s)?;let java=game::java(&app,&rt,&s).await?;game::vanilla(&app,&rt,&s).await?;
                let version="21.1.250".to_string();
                game::neoforge(&app,&rt,&s,&java,&version).await?;rt.log(&app,"Minecraft, Java 21 и NeoForge установлены. Укажите ник в профиле и нажмите ИГРАТЬ.");Ok(json!({}))},
            "play"=>{
                // Security boundary: check GitHub again inside the native PLAY command.
                // A stale frontend snapshot or direct IPC invocation cannot bypass the gate.
                let latest = check_github_update(&app, &rt).await?;
                match latest["status"].as_str() {
                    Some("current") => {},
                    Some("available") | Some("pending_asset") => {
                        bail!("Требуется обновление StudHub Launcher до версии {}. Откройте официальный GitHub Releases.", latest["version"].as_str().unwrap_or("новой"));
                    },
                    _ => bail!("Не удалось подтвердить актуальность лаунчера через GitHub. Проверьте интернет и нажмите «Повторить проверку»."),
                }
                let lock=acquire_game(&rt,&s)?;let a=auth::offline(&s.nickname)?;
                let _=drive::check(&app,&rt).await;
                let m=pack::sync(&app,&rt,&s).await?;*rt.game_lock.lock().unwrap()=Some(lock);
                match game::launch(&app,&rt,&s,&a,&m).await {Ok(pid)=>Ok(json!({"pid":pid})),Err(e)=>{rt.game_lock.lock().unwrap().take();Err(e)}}},
            "check_drive"=>drive::check(&app,&rt).await,
            "open_drive"=>{open::that(drive::FOLDER)?;Ok(json!({}))},
            "check_update"=>{
                let github = check_github_update(&app, &rt).await?;
                if !s.deployment.launcher_update_url.is_empty() {
                    let signed = app_update(&app,&rt,&s,false).await?;
                    Ok(json!({"github":github,"signed":signed}))
                } else {
                    Ok(github)
                }
            },
            "open_github_release"=>{
                let data=rt.github_update.lock().unwrap().clone();
                if data["status"]!="available" && data["status"]!="pending_asset" {bail!("Нет доступного обновления GitHub")}
                let url=github_updates::validated_release_url(data["release_url"].as_str().ok_or_else(||anyhow::anyhow!("Нет ссылки на релиз"))?)?;
                open::that(url)?;
                Ok(json!({}))
            },
            "download_update"=>{let _game=acquire_game(&rt,&s)?;app_update(&app,&rt,&s,true).await},
            _=>bail!("Неизвестная операция"),
        }
    }.await;
    match result {
        Ok(v) => {
            rt.changed(&app);
            Ok(v)
        }
        Err(e) => {
            rt.log(&app, &format!("Ошибка: {e:#}"));
            Err(e.to_string())
        }
    }
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            fs::create_dir_all(&dir)?;
            let file = dir.join("settings.json");
            let mut s = if file.exists() {
                serde_json::from_slice::<Settings>(&fs::read(file)?)?
            } else {
                Settings::default()
            };
            if s.game_dir.is_empty() {
                s.game_dir = app
                    .path()
                    .app_local_data_dir()?
                    .join("minecraft")
                    .to_string_lossy()
                    .into();
            }
            if s.deployment.server_address.is_empty() {
                s.deployment.server_address = "148.253.208.254:25589".into();
            }
            let deployment = std::env::current_exe()?
                .parent()
                .unwrap()
                .join("deployment.json");
            if deployment.exists() {
                let bytes = fs::read(deployment)?;
                if bytes.len() > 65536 {
                    return Err(anyhow::anyhow!("deployment.json превышает 64 КБ").into());
                }
                s.deployment = serde_json::from_slice(&bytes)?;
                validate_settings(&s)?;
            }
            let rt = Runtime {
                settings: Arc::new(Mutex::new(s.clone())),
                config_dir: dir,
                pid: Arc::new(AtomicU32::new(0)),
                busy: Arc::new(AtomicBool::new(false)),
                task: Arc::new(tokio::sync::Mutex::new(())),
                logs: Arc::new(Mutex::new(vec![])),
                game_lock: Arc::new(Mutex::new(None)),
                server: Arc::new(Mutex::new(
                    json!({"online":false,"error":"Подключение не проверено"}),
                )),
                news: Arc::new(Mutex::new(json!({"items":[]}))),
                update: Arc::new(Mutex::new(None)),
                github_update: Arc::new(Mutex::new(json!({"status":"checking"}))),
                drive: Arc::new(Mutex::new(
                    json!({"status":"unchecked","files":[],"changes":[]}),
                )),
            };
            app.manage(rt.clone());
            let handle = app.handle().clone();
            let startup_rt = rt.clone();
            tauri::async_runtime::spawn(async move {
                let rt = startup_rt;
                remote(&handle, &rt, &s).await;
                if s.auto_update {
                    if let Ok(_task) = rt.task.try_lock() {
                        rt.busy.store(true, Ordering::SeqCst);
                        rt.changed(&handle);
                        let _busy = BusyGuard {
                            rt: rt.clone(),
                            app: handle.clone(),
                        };
                        match acquire_game(&rt, &s) {
                            Ok(_lock) => {
                                if let Err(e) = pack::sync(&handle, &rt, &s).await {
                                    rt.log(&handle, &format!("Обновление сборки: {e}"));
                                }
                            }
                            Err(e) => rt.log(&handle, &e.to_string()),
                        }
                    }
                }
            });
            let status_handle = app.handle().clone();
            let status_rt = rt.clone();
            tauri::async_runtime::spawn(async move {
                let mut timer = tokio::time::interval(std::time::Duration::from_secs(15));
                timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                timer.tick().await;
                loop {
                    timer.tick().await;
                    refresh_server(&status_handle, &status_rt, &status_rt.settings()).await;
                }
            });
            // The first GitHub check starts immediately, independently of modpack installation.
            // Recheck every 30 minutes; native PLAY always performs a fresh fail-closed check.
            let github_handle = app.handle().clone();
            let github_rt = rt.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let _ = check_github_update(&github_handle, &github_rt).await;
                    tokio::time::sleep(std::time::Duration::from_secs(1800)).await;
                }
            });
            let update_handle = app.handle().clone();
            let update_rt = rt.clone();
            tauri::async_runtime::spawn(async move {
                let mut timer = tokio::time::interval(std::time::Duration::from_secs(60));
                timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut last = None;
                loop {
                    timer.tick().await;
                    if last.map_or(true, |t: std::time::Instant| t.elapsed().as_secs() >= 1800) {
                        if !update_rt.busy.load(Ordering::SeqCst)
                            && update_rt.pid.load(Ordering::SeqCst) == 0
                        {
                            automatic_update(&update_handle, &update_rt).await;
                            last = Some(std::time::Instant::now());
                        }
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let rt = window.state::<Runtime>();
                if rt.busy.load(Ordering::SeqCst) || rt.pid.load(Ordering::SeqCst) != 0 {
                    api.prevent_close();
                    let _ = window.emit(
                        "launcher-notice",
                        "Дождитесь завершения задачи или закройте Minecraft перед выходом.",
                    );
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            save_settings,
            open_folder,
            import_deployment,
            action
        ])
        .run(tauri::generate_context!())
        .expect("Не удалось запустить StudHub Launcher");
}
