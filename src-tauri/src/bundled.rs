use crate::{net, pack, Progress, Runtime, Settings};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::{fs, io::Read, path::Path};
use tauri::{AppHandle, Manager};

#[derive(Deserialize)]
struct Bundle {
    archive_sha256: String,
    archive_size: u64,
    archive_prefix: String,
    manifest: pack::Manifest,
}
fn bundle() -> Result<Bundle> {
    Ok(serde_json::from_str(include_str!("bundled-manifest.json"))?)
}
pub fn install(
    root: &Path,
    archive: &Path,
    mut progress: impl FnMut(Progress),
    mut log: impl FnMut(&str),
) -> Result<pack::Manifest> {
    let b = bundle()?;
    fs::create_dir_all(root)?;
    pack::reject_link(root)?;
    pack::reject_link(archive)?;
    if fs::metadata(archive)
        .context("Не найден встроенный архив. Переустановите StudHub Launcher")?
        .len()
        != b.archive_size
        || net::sha256(archive)? != b.archive_sha256
    {
        bail!("Встроенная сборка повреждена. Переустановите StudHub Launcher");
    }
    let m = b.manifest;
    pack::validate(root, &m)?;
    let old = pack::state(root)?;
    let pending = pack::pending(root, &old, &m)?;
    let meta = root.join(".studhub");
    pack::reject_link(&meta)?;
    fs::create_dir_all(&meta)?;
    let stage = tempfile::tempdir_in(&meta)?;
    let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)?;
    let total: u64 = pending.iter().map(|f| f.size).sum();
    let mut done = 0;
    for (i, f) in pending.iter().enumerate() {
        let name = format!(
            "{}{}",
            b.archive_prefix,
            f.path.strip_prefix("mods/").context("Путь вне mods")?
        );
        let mut entry = zip.by_name(&name)?;
        if entry.size() != f.size {
            bail!("Размер файла {} не совпадает с manifest", f.path);
        }
        let mut bytes = Vec::new();
        (&mut entry).take(f.size + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 != f.size {
            bail!("Неполный файл {}", f.path);
        }
        let dest = stage.path().join(i.to_string());
        net::atomic(&dest, &bytes)?;
        if net::sha256(&dest)? != f.sha256 {
            bail!("Не совпадает SHA-256 {}", f.path);
        }
        let ids = pack::metadata(&dest)?;
        if let Some(id) = &f.mod_id {
            if !ids.contains(id) {
                bail!("Нет заявленного modId {id} в {}", f.path);
            }
        }
        done += f.size;
        progress(Progress {
            stage: format!("Установка {}", f.path),
            done,
            total,
            speed: 0.0,
            eta: 0.0,
        });
    }
    pack::commit(root, old, &m, &pending, stage.path(), |s| log(s))?;
    progress(Progress {
        stage: "Сборка StudHub 1.0.2 проверена".into(),
        done: total,
        total,
        speed: 0.0,
        eta: 0.0,
    });
    Ok(m)
}
pub async fn sync(app: &AppHandle, rt: &Runtime, s: &Settings) -> Result<pack::Manifest> {
    let archive = app
        .path()
        .resource_dir()?
        .join("resources/studhub-pack-1.0.2.zip");
    let root = std::path::PathBuf::from(&s.game_dir);
    let app = app.clone();
    let rt = rt.clone();
    tokio::task::spawn_blocking(move || {
        install(
            &root,
            &archive,
            |p| rt.progress(&app, p),
            |line| rt.log(&app, line),
        )
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn archive() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/studhub-pack-1.0.2.zip")
    }
    #[test]
    fn real_pack_installs_repairs_and_preserves_extras() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::create_dir_all(root.join("mods")).unwrap();
        fs::write(root.join("mods/user-extra.jar"), b"player data").unwrap();
        let m = install(root, &archive(), |_| {}, |_| {}).unwrap();
        assert_eq!(m.version, "1.0.2");
        assert_eq!(m.files.len(), 48);
        for f in &m.files {
            assert_eq!(net::sha256(&root.join(&f.path)).unwrap(), f.sha256);
        }
        let kotlin = root.join("mods/kotlinforforge-5.12.0-all.jar");
        assert!(pack::metadata(&kotlin)
            .unwrap()
            .contains(&"kotlinforforge".into()));
        // Duplicate top-level mods are rejected, shared nested JarJar libraries are valid.
        fs::remove_file(root.join("mods/user-extra.jar")).unwrap();
        pack::compatibility(root, &m).unwrap();
        fs::write(&root.join(&m.files[0].path), b"corrupt").unwrap();
        fs::write(root.join("mods/extra.jar"), b"preserve").unwrap();
        install(root, &archive(), |_| {}, |_| {}).unwrap();
        assert_eq!(fs::read(root.join("mods/extra.jar")).unwrap(), b"preserve");
        assert_eq!(
            net::sha256(&root.join(&m.files[0].path)).unwrap(),
            m.files[0].sha256
        );
    }
    #[test]
    fn corrupt_archive_does_not_change_game_files() {
        let d = tempfile::tempdir().unwrap();
        let zip = d.path().join("broken.zip");
        fs::write(&zip, b"broken").unwrap();
        assert!(install(&d.path().join("game"), &zip, |_| {}, |_| {}).is_err());
        assert!(!d.path().join("game/mods").exists());
    }
}
