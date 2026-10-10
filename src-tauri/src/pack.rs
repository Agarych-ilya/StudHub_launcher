use crate::{net, Progress, Runtime, Settings};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use tauri::AppHandle;

#[derive(Clone, Serialize, Deserialize)]
pub struct PackFile {
    pub path: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default)]
    pub mod_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub minecraft: String,
    pub neoforge: String,
    pub files: Vec<PackFile>,
    #[serde(default)]
    pub incompatible_user_mods: Vec<String>,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Ownership {
    pub version: Option<String>,
    pub files: BTreeMap<String, String>,
    pub neoforge: Option<String>,
}
#[derive(Serialize)]
pub struct Mod {
    pub file: String,
    pub ids: Vec<String>,
    pub managed: bool,
    pub issue: Option<String>,
}
pub fn safe(root: &Path, name: &str) -> Result<PathBuf> {
    let parts: Vec<_> = name.split('/').collect();
    if name != "servers.dat"
        && (parts.len() < 2
            || ![
                "mods",
                "config",
                "defaultconfigs",
                "resourcepacks",
                "shaderpacks",
            ]
            .contains(&parts[0]))
    {
        bail!("Путь вне управляемых каталогов")
    }
    for p in &parts {
        let stem = p.split('.').next().unwrap().to_uppercase();
        if p.is_empty()
            || *p == "."
            || *p == ".."
            || p.ends_with([' ', '.'])
            || p.chars()
                .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
            || ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            bail!("Небезопасный путь: {name}")
        }
    }
    let mut out = root.to_path_buf();
    reject_link(root)?;
    for part in parts {
        out.push(part);
        reject_link(&out)?;
    }
    Ok(out)
}
pub fn reject_link(p: &Path) -> Result<()> {
    if fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        bail!("Символическая ссылка запрещена: {}", p.display())
    }
    Ok(())
}
pub fn state(root: &Path) -> Result<Ownership> {
    let p = root.join(".studhub/managed.json");
    reject_link(&root.join(".studhub"))?;
    reject_link(&p)?;
    if p.exists() {
        Ok(serde_json::from_slice(&fs::read(p)?)?)
    } else {
        Ok(Ownership::default())
    }
}
pub fn metadata(path: &Path) -> Result<Vec<String>> {
    let mut z = zip::ZipArchive::new(fs::File::open(path)?)?;
    let mut ids = Vec::new();
    let recognised = nested_metadata(&mut z, 0, &mut ids)?;
    if !recognised {
        bail!("Не найдено описание NeoForge/Forge-мода или библиотеки");
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}
fn nested_metadata<R: std::io::Read + std::io::Seek>(
    z: &mut zip::ZipArchive<R>,
    depth: usize,
    ids: &mut Vec<String>,
) -> Result<bool> {
    if depth > 8 {
        bail!("Слишком много уровней вложенных JAR");
    }
    let mut recognised = false;
    for name in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Ok(mut f) = z.by_name(name) {
            if f.size() > 1024 * 1024 {
                bail!("Метаданные мода слишком большие");
            }
            let mut s = String::new();
            f.read_to_string(&mut s)?;
            let v: toml::Value = s.parse()?;
            let mods = v
                .get("mods")
                .and_then(toml::Value::as_array)
                .context("Нет списка mods")?;
            let found = mods
                .iter()
                .filter_map(|m| {
                    m.get("modId")
                        .and_then(toml::Value::as_str)
                        .map(str::to_owned)
                })
                .collect::<Vec<_>>();
            if found.is_empty() {
                bail!("Нет modId");
            }
            ids.extend(found);
            recognised = true;
            break;
        }
    }
    // FML language providers and GAME/LIBRARY JARs do not require a mods.toml.
    if let Ok(mut f) = z.by_name("META-INF/MANIFEST.MF") {
        if f.size() > 1024 * 1024 {
            bail!("Manifest библиотеки слишком большой");
        }
        let mut s = String::new();
        f.read_to_string(&mut s)?;
        recognised |= s.lines().any(|l| {
            l.trim() == "FMLModType: LIBRARY"
                || l.trim() == "FMLModType: GAMELIBRARY"
                || l.trim() == "FMLModType: LANGPROVIDER"
        });
    }
    if depth == 0 && !ids.is_empty() {
        return Ok(true);
    }
    let mut nested = Vec::new();
    if let Ok(mut f) = z.by_name("META-INF/jarjar/metadata.json") {
        if f.size() > 1024 * 1024 {
            bail!("JarJar metadata слишком большие");
        }
        let mut s = String::new();
        f.read_to_string(&mut s)?;
        let v: serde_json::Value = serde_json::from_str(&s)?;
        for jar in v["jars"].as_array().context("Нет списка jarjar jars")? {
            nested.push(jar["path"].as_str().context("Нет пути jarjar")?.to_owned());
        }
    }
    if nested.len() > 128 {
        bail!("Слишком много вложенных JAR");
    }
    for name in nested {
        let data = {
            let mut f = z.by_name(&name)?;
            if f.size() > 128 * 1024 * 1024 {
                bail!("Вложенный JAR слишком большой");
            }
            let mut b = Vec::new();
            f.read_to_end(&mut b)?;
            b
        };
        let mut child = zip::ZipArchive::new(std::io::Cursor::new(data))?;
        recognised |= nested_metadata(&mut child, depth + 1, ids)?;
    }
    if !recognised && z.by_name("fabric.mod.json").is_ok() {
        bail!("Fabric-мод несовместим с NeoForge");
    }
    Ok(recognised)
}
pub fn mods(root: &Path) -> Result<Vec<Mod>> {
    let own = state(root)?;
    let mut list = Vec::new();
    let dir = root.join("mods");
    reject_link(&dir)?;
    if !dir.exists() {
        return Ok(list);
    }
    for e in fs::read_dir(dir)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.to_lowercase().ends_with(".jar") {
            continue;
        }
        let m = reject_link(&e.path()).and_then(|_| metadata(&e.path()));
        let (ids, issue) = match m {
            Ok(ids) => (ids, None),
            Err(e) => (vec![], Some(e.to_string())),
        };
        list.push(Mod {
            managed: own.files.contains_key(&format!("mods/{name}")),
            file: name,
            ids,
            issue,
        });
    }
    let mut seen = BTreeMap::new();
    for m in &list {
        for id in &m.ids {
            *seen.entry(id.clone()).or_insert(0) += 1;
        }
    }
    for m in &mut list {
        if m.ids.iter().any(|id| seen[id] > 1) {
            m.issue = Some("Повторяющийся modId: конфликт модов".into());
        }
    }
    list.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(list)
}
pub fn validate(root: &Path, m: &Manifest) -> Result<()> {
    if m.minecraft != "1.21.1" || !regex::Regex::new(r"^21\.1\.\d+$")?.is_match(&m.neoforge) {
        bail!("Требуются Minecraft 1.21.1 и NeoForge 21.1.x")
    }
    let mut seen = BTreeSet::new();
    for f in &m.files {
        safe(root, &f.path)?;
        if !seen.insert(f.path.to_lowercase()) {
            bail!("Повторяющийся путь")
        }
        if f.size == 0
            || f.size > 2_147_483_648
            || f.sha256.len() != 64
            || !f
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            bail!("Неверные размер или SHA-256")
        }
    }
    Ok(())
}
pub fn pending<'a>(root: &Path, old: &Ownership, m: &'a Manifest) -> Result<Vec<&'a PackFile>> {
    let mut out = Vec::new();
    for f in &m.files {
        let dest = safe(root, &f.path)?;
        if dest.exists() && !old.files.contains_key(&f.path) && net::sha256(&dest)? != f.sha256 {
            bail!("Файл {} добавлен пользователем: переместите его перед установкой управляемой версии",f.path)
        }
        if !dest.exists() || net::sha256(&dest)? != f.sha256 {
            out.push(f);
        }
    }
    Ok(out)
}
pub fn commit(
    root: &Path,
    old: Ownership,
    m: &Manifest,
    pending: &[&PackFile],
    stage: &Path,
    mut log: impl FnMut(&str),
) -> Result<()> {
    let meta = root.join(".studhub");
    reject_link(&meta)?;
    fs::create_dir_all(&meta)?;
    let mut journal = old.files.clone();
    for f in &m.files {
        journal.insert(f.path.clone(), f.sha256.clone());
    }
    net::atomic(
        &meta.join("managed.json"),
        &serde_json::to_vec(&Ownership {
            version: old.version.clone(),
            files: journal,
            neoforge: old.neoforge.clone(),
        })?,
    )?;
    for (i, f) in pending.iter().enumerate() {
        let dest = safe(root, &f.path)?;
        fs::create_dir_all(dest.parent().unwrap())?;
        let staged = stage.join(i.to_string());
        if net::sha256(&staged)? != f.sha256 {
            bail!("Хеш подготовленного файла изменился")
        }
        net::atomic(&dest, &fs::read(staged)?)?;
        if net::sha256(&dest)? != f.sha256 {
            bail!("Ошибка проверки после установки")
        }
    }
    let next = m
        .files
        .iter()
        .map(|f| (f.path.clone(), f.sha256.clone()))
        .collect::<BTreeMap<_, _>>();
    for (name, hash) in old.files {
        if !next.contains_key(&name) {
            let dest = safe(root, &name)?;
            if dest.exists() && net::sha256(&dest)? == hash {
                fs::remove_file(dest)?;
            } else if dest.exists() {
                log(&format!("Сохранён изменённый файл пользователя: {name}"));
            }
        }
    }
    net::atomic(
        &meta.join("managed.json"),
        &serde_json::to_vec(&Ownership {
            version: Some(m.version.clone()),
            files: next,
            neoforge: Some(m.neoforge.clone()),
        })?,
    )?;
    Ok(())
}
pub async fn sync(app: &AppHandle, rt: &Runtime, s: &Settings) -> Result<Manifest> {
    if s.deployment.manifest_url.is_empty() {
        return crate::bundled::sync(app, rt, s).await;
    }
    let root = Path::new(&s.game_dir);
    fs::create_dir_all(root)?;
    let c = net::client()?;
    let d = &s.deployment;
    let m: Manifest = serde_json::from_value(net::signed(
        net::json(&c, &d.manifest_url, &d.trusted_origins).await?,
        &d.manifest_public_key,
    )?)?;
    if m.neoforge != "21.1.250" {
        bail!("StudHub требует NeoForge 21.1.250");
    }
    validate(root, &m)?;
    let old = state(root)?;
    let meta = root.join(".studhub");
    reject_link(&meta)?;
    fs::create_dir_all(&meta)?;
    let stage = tempfile::tempdir_in(&meta)?;
    let pending = pending(root, &old, &m)?;
    let total = pending.iter().map(|f| f.size).sum();
    for f in &m.files {
        net::trusted(&f.url, &d.trusted_origins).await?;
    }
    let started = std::time::Instant::now();
    let mut done = 0;
    for (i, f) in pending.iter().enumerate() {
        let temp = stage.path().join(i.to_string());
        net::download(
            &c,
            &f.url,
            &d.trusted_origins,
            &temp,
            f.size,
            &f.sha256,
            false,
            |n| {
                let elapsed = started.elapsed().as_secs_f64().max(0.01);
                let speed = (done + n) as f64 / elapsed;
                rt.progress(
                    app,
                    Progress {
                        stage: format!("Загрузка {}", f.path),
                        done: done + n,
                        total,
                        speed,
                        eta: if speed > 0.0 {
                            (total - done - n) as f64 / speed
                        } else {
                            0.0
                        },
                    },
                );
            },
        )
        .await?;
        if f.path.starts_with("mods/") {
            let ids = metadata(&temp)?;
            if let Some(id) = &f.mod_id {
                if !ids.contains(id) {
                    bail!("JAR {} не содержит заявленный modId {id}", f.path)
                }
            }
        }
        done += f.size;
    }
    commit(root, old, &m, &pending, stage.path(), |line| {
        rt.log(app, line)
    })?;
    rt.progress(
        app,
        Progress {
            stage: "Сборка проверена".into(),
            done: total,
            total,
            speed: 0.0,
            eta: 0.0,
        },
    );
    Ok(m)
}
pub fn compatibility(root: &Path, m: &Manifest) -> Result<()> {
    for item in mods(root)? {
        if let Some(issue) = item.issue {
            bail!(
                "{}: {issue}. Файл сохранён; переместите его из mods.",
                item.file
            )
        }
        if !item.managed
            && m.incompatible_user_mods
                .iter()
                .any(|n| n.eq_ignore_ascii_case(&item.file))
        {
            bail!(
                "Несовместимый пользовательский мод: {}. Переместите его из mods.",
                item.file
            )
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_traversal_and_windows_names() {
        let root = tempfile::tempdir().unwrap();
        for n in [
            "mods/../evil",
            "../evil",
            "mods\\evil",
            "mods/CON.jar",
            "mods/foo:bar",
            "mods/x.",
            "mods//x",
            "mods/./x",
            "mods/a/../../x",
            "evil/x",
        ] {
            assert!(safe(root.path(), n).is_err(), "{n}");
        }
        assert!(safe(root.path(), "mods/my_mod-1.jar").is_ok());
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlink() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(other.path(), root.path().join("mods")).unwrap();
        assert!(safe(root.path(), "mods/x.jar").is_err());
    }
    #[test]
    fn hash_and_atomic_replace() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x");
        net::atomic(&p, b"hello").unwrap();
        assert_eq!(
            net::sha256(&p).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        net::atomic(&p, b"new").unwrap();
        assert_eq!(fs::read(p).unwrap(), b"new");
    }
    fn item(name: &str, bytes: &[u8]) -> PackFile {
        use sha2::{Digest, Sha256};
        PackFile {
            path: name.into(),
            url: "https://cdn.example/x".into(),
            size: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(bytes)),
            mod_id: None,
        }
    }
    fn manifest(files: Vec<PackFile>) -> Manifest {
        Manifest {
            version: "new".into(),
            minecraft: "1.21.1".into(),
            neoforge: "21.1.256".into(),
            files,
            incompatible_user_mods: vec![],
        }
    }
    #[test]
    fn update_repairs_managed_preserves_user_and_removes_obsolete() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::create_dir_all(root.join("mods")).unwrap();
        fs::write(root.join("mods/managed.jar"), b"corrupt").unwrap();
        fs::write(root.join("mods/old.jar"), b"old").unwrap();
        fs::write(root.join("mods/user.jar"), b"user").unwrap();
        fs::write(root.join("mods/edited.jar"), b"changed by player").unwrap();
        let old = Ownership {
            version: Some("old".into()),
            neoforge: Some("21.1.256".into()),
            files: BTreeMap::from([
                ("mods/managed.jar".into(), item("x", b"original").sha256),
                ("mods/old.jar".into(), item("x", b"old").sha256),
                ("mods/edited.jar".into(), item("x", b"original").sha256),
            ]),
        };
        let m = manifest(vec![item("mods/managed.jar", b"updated")]);
        let p = pending(root, &old, &m).unwrap();
        assert_eq!(p.len(), 1);
        let stage = tempfile::tempdir().unwrap();
        fs::write(stage.path().join("0"), b"updated").unwrap();
        commit(root, old, &m, &p, stage.path(), |_| {}).unwrap();
        assert_eq!(fs::read(root.join("mods/managed.jar")).unwrap(), b"updated");
        assert_eq!(fs::read(root.join("mods/user.jar")).unwrap(), b"user");
        assert!(root.join("mods/edited.jar").exists());
        assert!(!root.join("mods/old.jar").exists());
        let own = state(root).unwrap();
        assert_eq!(own.version.as_deref(), Some("new"));
        assert_eq!(own.files.len(), 1);
        assert!(pending(root, &own, &m).unwrap().is_empty());
    }
    #[test]
    fn refuses_to_overwrite_untracked_file() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("mods")).unwrap();
        fs::write(d.path().join("mods/x.jar"), b"user").unwrap();
        assert!(pending(
            d.path(),
            &Ownership::default(),
            &manifest(vec![item("mods/x.jar", b"new")])
        )
        .is_err());
        assert_eq!(fs::read(d.path().join("mods/x.jar")).unwrap(), b"user");
    }
    #[test]
    fn interrupted_commit_remains_repairable() {
        let d = tempfile::tempdir().unwrap();
        let m = manifest(vec![item("mods/x.jar", b"new")]);
        let old = Ownership::default();
        let p = pending(d.path(), &old, &m).unwrap();
        let stage = tempfile::tempdir().unwrap();
        fs::write(stage.path().join("0"), b"bad").unwrap();
        assert!(commit(d.path(), old, &m, &p, stage.path(), |_| {}).is_err());
        let own = state(d.path()).unwrap();
        assert!(own.files.contains_key("mods/x.jar"));
        assert_eq!(pending(d.path(), &own, &m).unwrap().len(), 1);
    }
}
