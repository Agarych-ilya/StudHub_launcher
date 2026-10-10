use crate::{auth::Account, net, pack, Progress, Runtime, Settings};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
};
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, BufReader};

pub const MC: &str = "1.21.1";
fn native_command(exe: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(exe);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    #[cfg(not(windows))]
    let _ = &mut cmd;
    cmd
}
fn installation_complete(root: &Path, version: &str) -> Result<bool> {
    let marker = root.join(format!(".studhub/neoforge-{version}.complete.json"));
    if !marker.exists() {
        return Ok(false);
    }
    let files: BTreeMap<String, String> = serde_json::from_slice(&fs::read(marker)?)?;
    if files.len() < 5 {
        return Ok(false);
    }
    for (name, hash) in files {
        let p = path(root, &name)?;
        if !p.is_file() || net::sha256(&p)? != hash {
            return Ok(false);
        }
    }
    Ok(true)
}
fn clear_derived(root: &Path, profile: &Value) -> Result<()> {
    for name in [
        "MAPPINGS",
        "MOJMAPS",
        "MERGED_MAPPINGS",
        "MC_UNPACKED",
        "MC_SLIM",
        "MC_EXTRA",
        "MC_SRG",
        "PATCHED",
    ] {
        if let Some(value) = profile["data"][name]["client"].as_str() {
            if let Some(coordinate) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
                let file = path(&root.join("libraries"), &maven(coordinate)?)?;
                if file.is_file() {
                    fs::remove_file(file)?;
                }
            }
        }
    }
    Ok(())
}
pub fn os() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}
pub fn sources() -> Vec<String> {
    [
        "https://piston-meta.mojang.com",
        "https://piston-data.mojang.com",
        "https://launchermeta.mojang.com",
        "https://launcher.mojang.com",
        "https://libraries.minecraft.net",
        "https://resources.download.minecraft.net",
        "https://maven.neoforged.net",
    ]
    .iter()
    .map(|x| x.to_string())
    .collect()
}
pub fn path(root: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.contains(['\\', ':'])
        || name.starts_with('/')
        || name
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        bail!("Небезопасный путь установщика")
    }
    let mut p = root.to_owned();
    pack::reject_link(root)?;
    for part in name.split('/') {
        p.push(part);
        pack::reject_link(&p)?;
    }
    Ok(p)
}
fn hash1(p: &Path) -> Result<String> {
    let mut f = fs::File::open(p)?;
    let mut h = Sha1::new();
    let mut b = [0; 65536];
    loop {
        let n = f.read(&mut b)?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(hex::encode(h.finalize()))
}
pub async fn artifact(c: &reqwest::Client, v: &Value, dest: &Path) -> Result<()> {
    let hash = v["sha1"].as_str().context("Нет SHA-1 официального файла")?;
    let size = v["size"].as_u64().context("Нет размера файла")?;
    if dest.exists() && fs::metadata(dest)?.len() == size && hash1(dest)? == hash {
        return Ok(());
    }
    net::download(
        c,
        v["url"].as_str().context("Нет URL")?,
        &sources(),
        dest,
        size,
        hash,
        true,
        |_| {},
    )
    .await
}
fn runtime_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "aarch64") {
            "windows-arm64"
        } else {
            "windows-x64"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-os-arm64"
        } else {
            "mac-os"
        }
    } else {
        if cfg!(target_arch = "aarch64") {
            "linux-arm64"
        } else {
            "linux"
        }
    }
}
pub async fn java(app: &AppHandle, rt: &Runtime, s: &Settings) -> Result<PathBuf> {
    if !s.java_path.is_empty() {
        let p = PathBuf::from(&s.java_path);
        verify_java(&p).await?;
        return Ok(p);
    }
    let root = Path::new(&s.game_dir).join("runtime/java21");
    let exe = if cfg!(target_os = "windows") {
        "bin/java.exe"
    } else if cfg!(target_os = "macos") {
        "jre.bundle/Contents/Home/bin/java"
    } else {
        "bin/java"
    };
    let p = root.join(exe);
    // Revalidate every runtime file, even on a previously installed runtime.
    rt.log(app, "Проверка Java 21 из официального каталога Mojang");
    let c = net::client()?;
    let all=net::json(&c,"https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json",&sources()).await?;
    let index = &all[runtime_platform()]["java-runtime-delta"][0];
    let url = index["manifest"]["url"]
        .as_str()
        .context("Java 21 недоступна для этой платформы; выберите Java 21 вручную")?;
    let manifest = net::json(&c, url, &sources()).await?;
    let files = manifest["files"]
        .as_object()
        .context("Нет списка Java runtime")?;
    fs::create_dir_all(&root)?;
    let mut links = Vec::new();
    for (i, (name, item)) in files.iter().enumerate() {
        let dest = path(&root, name)?;
        match item["type"].as_str() {
            Some("directory") => fs::create_dir_all(&dest)?,
            Some("file") => {
                artifact(&c, &item["downloads"]["raw"], &dest).await?;
                #[cfg(unix)]
                if item["executable"] == true {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))?;
                }
            }
            Some("link") => {
                // Mojang links are reconstructed as copies; never follow an arbitrary target.
                let target = item["target"].as_str().context("Нет link target")?;
                links.push((dest, target.to_owned()));
            }
            _ => bail!("Неизвестный тип Java runtime"),
        }
        rt.progress(
            app,
            Progress::steps("Подготовка Java 21", i as u64 + 1, files.len() as u64),
        );
    }
    for (dest, target) in links {
        let resolved = dest.parent().unwrap().join(target).canonicalize()?;
        if !resolved.starts_with(root.canonicalize()?) || !resolved.is_file() {
            bail!("Небезопасная ссылка Java runtime")
        }
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::copy(resolved, &dest)?;
    }
    verify_java(&p).await?;
    Ok(p)
}
async fn verify_java(p: &Path) -> Result<()> {
    let r = native_command(p)
        .arg("-version")
        .output()
        .await
        .context("Не удалось запустить Java")?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&r.stderr),
        String::from_utf8_lossy(&r.stdout)
    );
    if !r.status.success() || !regex::Regex::new(r#"version "21[."]"#)?.is_match(&text) {
        bail!("Требуется Java 21. Получено: {}", text.trim())
    }
    Ok(())
}
pub fn allowed(v: &Value, s: &Settings) -> bool {
    let Some(rules) = v.get("rules").and_then(Value::as_array) else {
        return true;
    };
    let mut allow = false;
    for r in rules {
        if let Some(name) = r["os"]["name"].as_str() {
            if name != os() {
                continue;
            }
        }
        if let Some(arch) = r["os"]["arch"].as_str() {
            let current = if cfg!(target_arch = "aarch64") {
                "aarch64"
            } else {
                "x86_64"
            };
            if arch != current {
                continue;
            }
        }
        if let Some(version) = r["os"]["version"].as_str() {
            // Only supported desktop OSes; evaluate Windows version rules against 10/11 NT major version.
            let current = if os() == "windows" { "10.0" } else { "" };
            if regex::Regex::new(version).map_or(true, |re| !re.is_match(current)) {
                continue;
            }
        }
        if let Some(features) = r["features"].as_object() {
            if features.iter().any(|(k, v)| {
                let enabled = match k.as_str() {
                    "is_quick_play_multiplayer" => {
                        s.auto_connect && !s.deployment.server_address.is_empty()
                    }
                    "has_quick_plays_support" => true,
                    _ => false,
                };
                v.as_bool() != Some(enabled)
            }) {
                continue;
            }
        }
        allow = r["action"] == "allow";
    }
    allow
}
pub fn args(v: &Value, s: &Settings) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(a) = v.as_array() {
        for item in a {
            if let Some(value) = item.as_str() {
                result.push(value.into());
            } else if allowed(item, s) {
                if let Some(value) = item["value"].as_str() {
                    result.push(value.into())
                } else if let Some(values) = item["value"].as_array() {
                    result.extend(values.iter().filter_map(|x| x.as_str().map(str::to_owned)))
                }
            }
        }
    }
    result
}
pub fn maven(name: &str) -> Result<String> {
    let (name, extension) = name.rsplit_once('@').unwrap_or((name, "jar"));
    if !extension.bytes().all(|b| b.is_ascii_alphanumeric()) {
        bail!("Неверное расширение Maven")
    }
    let parts = name.split(':').collect::<Vec<_>>();
    if parts.len() < 3 || parts.len() > 4 {
        bail!("Неверное Maven имя")
    }
    Ok(format!(
        "{}/{}/{}/{}-{}{}.{}",
        parts[0].replace('.', "/"),
        parts[1],
        parts[2],
        parts[1],
        parts[2],
        if parts.len() == 4 {
            format!("-{}", parts[3])
        } else {
            String::new()
        },
        extension
    ))
}
async fn libraries(
    app: &AppHandle,
    rt: &Runtime,
    s: &Settings,
    profile: &Value,
) -> Result<Vec<(String, PathBuf)>> {
    let root = Path::new(&s.game_dir);
    let c = net::client()?;
    let mut result = Vec::new();
    let list = profile["libraries"].as_array().context("Нет libraries")?;
    let native_dir = root.join("natives");
    pack::reject_link(&native_dir)?;
    fs::create_dir_all(&native_dir)?;
    for (i, lib) in list.iter().enumerate() {
        if !allowed(lib, s) {
            continue;
        }
        let name = lib["name"].as_str().context("Нет имени библиотеки")?;
        if let Some(artifact_v) = lib["downloads"].get("artifact") {
            let dest = path(
                &root.join("libraries"),
                artifact_v["path"].as_str().context("Нет library path")?,
            )?;
            artifact(&c, artifact_v, &dest).await?;
            result.push((name.into(), dest.clone()));
            if name.contains(":natives-") {
                extract_natives(&dest, &native_dir)?;
            }
        } else {
            let dest = path(&root.join("libraries"), &maven(name)?)?;
            if !dest.exists() {
                bail!("NeoForge не подготовил библиотеку {name}. Повторите установку.")
            }
            result.push((name.into(), dest));
        }
        if let Some(classifier) = lib["natives"][os()].as_str() {
            let classifier = classifier.replace(
                "${arch}",
                if cfg!(target_pointer_width = "64") {
                    "64"
                } else {
                    "32"
                },
            );
            let a = &lib["downloads"]["classifiers"][&classifier];
            let dest = path(
                &root.join("libraries"),
                a["path"].as_str().context("Нет native path")?,
            )?;
            artifact(&c, a, &dest).await?;
            extract_natives(&dest, &native_dir)?;
        }
        rt.progress(
            app,
            Progress::steps("Проверка библиотек", i as u64 + 1, list.len() as u64),
        );
    }
    Ok(result)
}
pub fn extract_natives(jar: &Path, dest: &Path) -> Result<()> {
    let mut z = zip::ZipArchive::new(fs::File::open(jar)?)?;
    for i in 0..z.len() {
        let mut f = z.by_index(i)?;
        let name = f.name().to_string();
        if f.is_dir()
            || !(name.ends_with(".dll")
                || name.ends_with(".so")
                || name.ends_with(".dylib")
                || name.ends_with(".jnilib"))
        {
            continue;
        }
        if f.size() > 100 * 1024 * 1024 {
            bail!("Native слишком большой")
        }
        let p = path(dest, &name)?;
        fs::create_dir_all(p.parent().unwrap())?;
        let mut out = tempfile::NamedTempFile::new_in(p.parent().unwrap())?;
        std::io::copy(&mut f, &mut out)?;
        out.as_file().sync_all()?;
        out.persist(p).map_err(|e| e.error)?;
    }
    Ok(())
}
pub async fn vanilla(app: &AppHandle, rt: &Runtime, s: &Settings) -> Result<Value> {
    let root = Path::new(&s.game_dir);
    fs::create_dir_all(root)?;
    let c = net::client()?;
    rt.log(app, "Установка и проверка Minecraft 1.21.1");
    let index = net::json(
        &c,
        "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
        &sources(),
    )
    .await?;
    let item = index["versions"]
        .as_array()
        .context("Нет versions")?
        .iter()
        .find(|v| v["id"] == MC)
        .context("Версия не найдена")?;
    let profile = net::json(
        &c,
        item["url"].as_str().context("Нет version URL")?,
        &sources(),
    )
    .await?;
    let dir = root.join(format!("versions/{MC}"));
    fs::create_dir_all(&dir)?;
    net::atomic(
        &dir.join(format!("{MC}.json")),
        &serde_json::to_vec(&profile)?,
    )?;
    artifact(
        &c,
        &profile["downloads"]["client"],
        &dir.join(format!("{MC}.jar")),
    )
    .await?;
    libraries(app, rt, s, &profile).await?;
    let ai = &profile["assetIndex"];
    let id = ai["id"].as_str().context("Нет asset index id")?;
    if !id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        bail!("Неверный asset index id")
    }
    let index_file = root.join(format!("assets/indexes/{id}.json"));
    artifact(&c, ai, &index_file).await?;
    let assets: Value = serde_json::from_slice(&fs::read(index_file)?)?;
    let objects = assets["objects"].as_object().context("Нет asset objects")?;
    let mut tasks = tokio::task::JoinSet::new();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(12));
    for item in objects.values() {
        let hash = item["hash"].as_str().context("Нет asset hash")?.to_owned();
        if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("Неверный asset hash")
        }
        let size = item["size"].as_u64().context("Нет asset size")?;
        let dest = root.join(format!("assets/objects/{}/{}", &hash[..2], hash));
        let c = c.clone();
        let sem = semaphore.clone();
        tasks.spawn(async move {
            let _permit=sem.acquire_owned().await?;
            let a=json!({"url":format!("https://resources.download.minecraft.net/{}/{}",&hash[..2],hash),"sha1":hash,"size":size});
            artifact(&c,&a,&dest).await
        });
    }
    let mut done = 0;
    while let Some(result) = tasks.join_next().await {
        result??;
        done += 1;
        rt.progress(
            app,
            Progress::steps("Загрузка ресурсов Minecraft", done, objects.len() as u64),
        );
    }
    Ok(profile)
}
pub async fn neoforge(
    app: &AppHandle,
    rt: &Runtime,
    s: &Settings,
    java: &Path,
    version: &str,
) -> Result<Value> {
    if !regex::Regex::new(r"^21\.1\.\d+$")?.is_match(version) {
        bail!("Неверная версия NeoForge")
    }
    let root = Path::new(&s.game_dir);
    let id = format!("neoforge-{version}");
    let p = root.join(format!("versions/{id}/{id}.json"));
    if p.exists() && installation_complete(root, version).unwrap_or(false) {
        return Ok(serde_json::from_slice(&fs::read(p)?)?);
    }
    let url=format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar");
    let c = net::client()?;
    net::trusted(&url, &sources()).await?;
    let hash = net::checked(c.get(format!("{url}.sha1")).send().await?)
        .await?
        .text()
        .await?;
    let expected = hash
        .split_whitespace()
        .next()
        .context("Нет installer SHA-1")?;
    let data = net::checked(c.get(&url).send().await?)
        .await?
        .bytes()
        .await?;
    if data.len() > 64 * 1024 * 1024 || hex::encode(Sha1::digest(&data)) != expected {
        bail!("Не совпадает SHA-1 установщика NeoForge")
    }
    let jar = root.join(format!(".studhub/neoforge-{version}-installer.jar"));
    net::atomic(&jar, &data)?;
    // Download official dependencies with the launcher's TLS/proxy/retry implementation.
    // The Java installer then runs the upstream processors against verified local artifacts.
    let profiles = {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&data))?;
        let mut profiles = Vec::new();
        for name in ["install_profile.json", "version.json"] {
            let mut source = archive.by_name(name)?;
            if source.size() > 8 * 1024 * 1024 {
                bail!("Профиль установщика слишком большой")
            }
            let mut content = String::new();
            source.read_to_string(&mut content)?;
            profiles.push(serde_json::from_str::<Value>(&content)?);
        }
        profiles
    };
    for profile in &profiles {
        if let Some(libs) = profile["libraries"].as_array() {
            for lib in libs {
                let a = &lib["downloads"]["artifact"];
                if a["url"].as_str().map_or(true, str::is_empty) {
                    continue;
                }
                let dest = path(
                    &root.join("libraries"),
                    a["path"].as_str().context("Нет NeoForge library path")?,
                )?;
                rt.log(
                    app,
                    &format!(
                        "Зависимость NeoForge: {}",
                        lib["name"].as_str().unwrap_or("library")
                    ),
                );
                artifact(&c, a, &dest).await?;
            }
        }
    }
    let proxy = std::env::var("HTTPS_PROXY")
        .or_else(|_| std::env::var("https_proxy"))
        .ok();
    let mut jvm_network = Vec::new();
    if let Some(proxy) = proxy {
        let parsed = url::Url::parse(&proxy)?;
        if let Some(host) = parsed.host_str() {
            let port = parsed
                .port_or_known_default()
                .context("Нет порта HTTPS proxy")?;
            for protocol in ["http", "https"] {
                jvm_network.push(format!("-D{protocol}.proxyHost={host}"));
                jvm_network.push(format!("-D{protocol}.proxyPort={port}"));
            }
        }
    }
    // Honor explicitly configured enterprise/test CAs without disabling TLS verification
    // or changing the checksum-verified Mojang runtime.
    let mut custom_trust = None;
    if let Ok(cert_file) = std::env::var("SSL_CERT_FILE") {
        let certificates = fs::read_to_string(cert_file)?;
        if certificates.len() > 1024 * 1024 {
            bail!("CA bundle слишком большой")
        }
        let home = java
            .parent()
            .and_then(Path::parent)
            .context("Нет каталога Java")?;
        let temp = tempfile::tempdir_in(root.join(".studhub"))?;
        let trust = temp.path().join("truststore");
        fs::copy(home.join("lib/security/cacerts"), &trust)?;
        let keytool = home.join(if cfg!(windows) {
            "bin/keytool.exe"
        } else {
            "bin/keytool"
        });
        let pattern =
            regex::Regex::new(r"(?s)-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----")?;
        let mut count = 0;
        for certificate in pattern.find_iter(&certificates) {
            let pem = temp.path().join(format!("ca-{count}.pem"));
            fs::write(&pem, certificate.as_str())?;
            let status = native_command(&keytool)
                .args([
                    "-importcert",
                    "-noprompt",
                    "-alias",
                    &format!("studhub-env-{count}"),
                    "-file",
                ])
                .arg(&pem)
                .arg("-keystore")
                .arg(&trust)
                .args(["-storepass", "changeit"])
                .output()
                .await?;
            if !status.status.success() {
                bail!(
                    "Не удалось импортировать настроенный CA: {}",
                    String::from_utf8_lossy(&status.stderr)
                )
            }
            count += 1;
        }
        if count == 0 {
            bail!("SSL_CERT_FILE не содержит PEM-сертификатов")
        }
        jvm_network.push(format!("-Djavax.net.ssl.trustStore={}", trust.display()));
        jvm_network.push("-Djavax.net.ssl.trustStorePassword=changeit".into());
        custom_trust = Some(temp);
    }
    // Upstream processors use CREATE_NEW for some outputs. A failed or damaged
    // installation must regenerate its derived Maven files, not collide with them.
    clear_derived(root, &profiles[0])?;
    if !root.join("launcher_profiles.json").exists() {
        net::atomic(&root.join("launcher_profiles.json"), b"{\"profiles\":{}}")?;
    }
    rt.log(
        app,
        &format!("Запуск официального установщика NeoForge {version}"),
    );
    let mut child = native_command(java)
        .args(&jvm_network)
        .arg("-jar")
        .arg(&jar)
        .arg("--installClient")
        .arg(root)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let a = app.clone();
    let r = rt.clone();
    let stdout = tokio::spawn(async move {
        let mut lines = BufReader::new(out).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            r.log(&a, &l)
        }
    });
    let a = app.clone();
    let r = rt.clone();
    let stderr = tokio::spawn(async move {
        let mut lines = BufReader::new(err).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            r.log(&a, &l)
        }
    });
    let status = child.wait().await?;
    stdout.await?;
    stderr.await?;
    drop(custom_trust);
    if !status.success() {
        bail!("Установщик NeoForge завершился с ошибкой {status}. Откройте журнал.")
    }
    if !p.exists() {
        bail!("NeoForge не создал профиль {id}")
    }
    let profile: Value = serde_json::from_slice(&fs::read(&p)?)?;
    libraries(app, rt, s, &profile).await?;
    let mut completed = BTreeMap::new();
    completed.insert(format!("versions/{id}/{id}.json"), net::sha256(&p)?);
    for name in ["PATCHED", "MC_EXTRA", "MC_SRG"] {
        let coordinate = profiles[0]["data"][name]["client"]
            .as_str()
            .context("Нет выходного файла NeoForge")?;
        let coordinate = coordinate
            .strip_prefix('[')
            .and_then(|v| v.strip_suffix(']'))
            .context("Неверный путь выходного файла")?;
        let relative = format!("libraries/{}", maven(coordinate)?);
        let output = path(root, &relative)?;
        if !output.is_file() || fs::metadata(&output)?.len() == 0 {
            bail!("NeoForge не подготовил {relative}")
        }
        completed.insert(relative, net::sha256(&output)?);
    }
    let universal = format!(
        "libraries/{}",
        maven(&format!("net.neoforged:neoforge:{version}:universal"))?
    );
    completed.insert(universal.clone(), net::sha256(&path(root, &universal)?)?);
    net::atomic(
        &root.join(format!(".studhub/neoforge-{version}.complete.json")),
        &serde_json::to_vec(&completed)?,
    )?;
    Ok(profile)
}
fn key(name: &str) -> String {
    let p = name.split(':').collect::<Vec<_>>();
    format!(
        "{}:{}:{}",
        p.first().unwrap_or(&""),
        p.get(1).unwrap_or(&""),
        p.get(3).unwrap_or(&"")
    )
}
pub fn substitute(arg: &str, values: &BTreeMap<&str, String>) -> Result<String> {
    let mut out = arg.to_owned();
    for (k, v) in values {
        out = out.replace(&format!("\u{24}{{{k}}}"), v);
    }
    if out.contains("\u{24}{") {
        bail!("Не подставлен аргумент запуска: {out}")
    }
    Ok(out)
}
pub async fn launch(
    app: &AppHandle,
    rt: &Runtime,
    s: &Settings,
    a: &Account,
    m: &pack::Manifest,
) -> Result<u32> {
    let root = Path::new(&s.game_dir);
    pack::compatibility(root, m)?;
    let java = java(app, rt, s).await?;
    let parent = vanilla(app, rt, s).await?;
    let child = neoforge(app, rt, s, &java, &m.neoforge).await?;
    let mut cp = BTreeMap::new();
    for profile in [&parent, &child] {
        for (name, p) in libraries(app, rt, s, profile).await? {
            cp.insert(key(&name), p);
        }
    }
    // FML's ProductionClientProvider discovers the verified srg/extra/patched JARs
    // in library_directory. Adding the obfuscated vanilla client to the bootstrap
    // classpath would introduce an additional conflicting Minecraft module.
    let cp = cp.into_values().collect::<Vec<_>>();
    let cp = std::env::join_paths(cp)?.to_string_lossy().into_owned();
    let asset_index = parent["assetIndex"]["id"]
        .as_str()
        .context("Нет assetIndex")?
        .to_owned();
    let mut v = BTreeMap::new();
    v.insert("auth_player_name", a.name.clone());
    v.insert("auth_uuid", a.id.clone());
    v.insert("auth_access_token", a.access_token.clone());
    v.insert("auth_xuid", a.xuid.clone());
    v.insert("clientid", String::new());
    v.insert("version_name", format!("neoforge-{}", m.neoforge));
    v.insert("version_type", "release".into());
    v.insert("game_directory", root.to_string_lossy().into());
    v.insert("assets_root", root.join("assets").to_string_lossy().into());
    v.insert("assets_index_name", asset_index);
    v.insert("user_type", "legacy".into());
    v.insert("user_properties", "{}".into());
    v.insert(
        "natives_directory",
        root.join("natives").to_string_lossy().into(),
    );
    v.insert("launcher_name", "StudHub".into());
    v.insert("launcher_version", env!("CARGO_PKG_VERSION").into());
    v.insert("classpath", cp);
    v.insert(
        "classpath_separator",
        if cfg!(windows) { ";" } else { ":" }.into(),
    );
    v.insert(
        "library_directory",
        root.join("libraries").to_string_lossy().into(),
    );
    v.insert(
        "quickPlayPath",
        root.join(".studhub/quickplay.json")
            .to_string_lossy()
            .into(),
    );
    v.insert("quickPlayMultiplayer", s.deployment.server_address.clone());
    let mut command = vec![
        format!("-Xmx{}M", memory_limit(s.ram_mb)),
        "-Xms512M".into(),
    ];
    for profile in [&parent, &child] {
        command.extend(
            args(&profile["arguments"]["jvm"], s)
                .iter()
                .map(|a| substitute(a, &v))
                .collect::<Result<Vec<_>>>()?,
        );
    }
    command.push(
        child["mainClass"]
            .as_str()
            .context("Нет mainClass NeoForge")?
            .into(),
    );
    for profile in [&parent, &child] {
        command.extend(
            args(&profile["arguments"]["game"], s)
                .iter()
                .map(|a| substitute(a, &v))
                .collect::<Result<Vec<_>>>()?,
        );
    }
    let mut proc = native_command(&java)
        .args(&command)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let pid = proc.id().context("Нет PID")?;
    rt.pid.store(pid, std::sync::atomic::Ordering::SeqCst);
    rt.log(
        app,
        &format!("Minecraft запущен, PID {pid}. Игрок: {}", a.name),
    );
    rt.changed(app);
    let stdout = proc.stdout.take().unwrap();
    let stderr = proc.stderr.take().unwrap();
    let r = rt.clone();
    let a_app = app.clone();
    let token = a.access_token.clone();
    tokio::spawn(async move {
        let r1 = r.clone();
        let a1 = a_app.clone();
        let token1 = token.clone();
        let out = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                r1.log(&a1, &l.replace(&token1, "[REDACTED]"))
            }
        });
        let r2 = r.clone();
        let a2 = a_app.clone();
        let err = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                r2.log(&a2, &l.replace(&token, "[REDACTED]"))
            }
        });
        let status = proc.wait().await;
        let _ = out.await;
        let _ = err.await;
        r.pid.store(0, std::sync::atomic::Ordering::SeqCst);
        r.game_lock.lock().unwrap().take();
        r.log(&a_app, &format!("Minecraft завершён: {status:?}"));
        r.changed(&a_app);
    });
    Ok(pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_installer_profile_is_not_success() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("versions/neoforge-21.1.256");
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("neoforge-21.1.256.json"), b"{}").unwrap();
        assert!(!installation_complete(d.path(), "21.1.256").unwrap());
    }
    #[test]
    fn repairing_installer_clears_derived_files_but_preserves_user_mods() {
        let d = tempfile::tempdir().unwrap();
        let p = d
            .path()
            .join("libraries/net/minecraft/client/1/client-1-mappings.txt");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, b"partial").unwrap();
        let user = d.path().join("mods/user.jar");
        fs::create_dir_all(user.parent().unwrap()).unwrap();
        fs::write(&user, b"user").unwrap();
        clear_derived(
            d.path(),
            &json!({"data":{"MOJMAPS":{"client":"[net.minecraft:client:1:mappings@txt]"}}}),
        )
        .unwrap();
        assert!(!p.exists());
        assert_eq!(fs::read(user).unwrap(), b"user");
    }
    #[test]
    fn replacement_and_unknown_placeholder() {
        let mut v = BTreeMap::new();
        v.insert("clientid", "abc".into());
        assert_eq!(substitute("\u{24}{clientid}", &v).unwrap(), "abc");
        assert!(substitute("\u{24}{unknown}", &v).is_err());
    }
    #[test]
    fn maven_paths() {
        assert_eq!(
            maven("net.neoforged:foo:1:client").unwrap(),
            "net/neoforged/foo/1/foo-1-client.jar"
        );
    }
    #[test]
    fn rules_are_evaluated() {
        let s = Settings::default();
        assert!(!allowed(
            &json!({"rules":[{"action":"allow","features":{"is_demo_user":true}}]}),
            &s
        ));
        assert!(allowed(&json!({}), &s));
    }
}

fn auto_memory(total_mb: u64) -> u32 {
    ((total_mb / 2 / 512 * 512).clamp(2048, 8192)) as u32
}
pub fn memory_limit(requested: u32) -> u32 {
    if requested != 0 {
        return requested;
    }
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    auto_memory(system.total_memory() / 1024 / 1024)
}
#[cfg(test)]
mod memory_tests {
    use super::*;
    #[test]
    fn leaves_memory_for_system() {
        assert_eq!(auto_memory(4096), 2048);
        assert_eq!(auto_memory(8192), 4096);
        assert_eq!(auto_memory(16384), 8192);
        assert_eq!(auto_memory(32768), 8192);
        assert_eq!(memory_limit(3072), 3072);
    }
}
