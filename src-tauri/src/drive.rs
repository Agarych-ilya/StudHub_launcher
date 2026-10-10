//! Anonymous check of the configured public folder. Google can change its web format;
//! parsing failures are errors, never an empty folder or a successful check.
use crate::{net, Runtime};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;

pub const FOLDER: &str = "https://drive.google.com/drive/folders/1iGRUgwkS9e6HHqZ9ItJ50msGRBld9lGF";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct File {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub modified: u64,
}
fn baseline() -> Vec<File> {
    vec![File {
        id: "1EHpLI0yRDJtXyF8369jy3GTC1KKY41OI".into(),
        name: "Сборка модов 1.0.2.zip".into(),
        size: 187653771,
        modified: 1791124687126,
    }]
}
fn decode(raw: &str) -> Result<String> {
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let e = chars.next().context("Неполный escape Drive")?;
        match e {
            'x' | 'u' => {
                let n = if e == 'x' { 2 } else { 4 };
                let digits: String = chars.by_ref().take(n).collect();
                if digits.len() != n {
                    bail!("Неполный escape Drive")
                }
                out.push(
                    char::from_u32(u32::from_str_radix(&digits, 16)?)
                        .context("Неверный символ Drive")?,
                );
            }
            '\\' | '\'' | '"' | '/' => out.push(e),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            _ => bail!("Неизвестный escape Drive"),
        }
    }
    Ok(out)
}
fn parse(html: &str) -> Result<Vec<File>> {
    let re = regex::Regex::new(r"window\['_DRIVE_ivd'\]\s*=\s*'((?:\\.|[^'\\])*)'")?;
    let cap = re
        .captures(html)
        .context("Не удалось прочитать список Drive: проверьте общий доступ или формат страницы")?;
    let data: Value = serde_json::from_str(&decode(&cap[1])?)?;
    let records = data[0]
        .as_array()
        .context("Drive не вернул список файлов")?;
    // Public web listings may be paginated. Do not report a partial listing as current.
    if records.len() >= 50 {
        bail!("В папке слишком много файлов для полной проверки. Откройте Drive")
    }
    let mut files = Vec::new();
    for row in records {
        let id = row[0].as_str().context("Нет ID файла Drive")?;
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            bail!("Некорректный ID Drive")
        }
        let name = row[2].as_str().context("Нет имени файла Drive")?;
        let mime = row[3].as_str().context("Нет типа файла Drive")?;
        let size = if mime == "application/vnd.google-apps.folder" {
            0
        } else {
            row[13]
                .as_u64()
                .or_else(|| row[13].as_str().and_then(|s| s.parse().ok()))
                .context("Нет размера файла Drive")?
        };
        files.push(File {
            id: id.into(),
            name: name.into(),
            size,
            modified: row[9].as_u64().context("Нет даты изменения Drive")?,
        });
    }
    files.sort_by(|a, b| a.id.cmp(&b.id));
    if files.windows(2).any(|w| w[0].id == w[1].id) {
        bail!("Повторяющиеся ID Drive")
    }
    Ok(files)
}
fn diff(old: &[File], new: &[File]) -> Vec<String> {
    let old: BTreeMap<_, _> = old.iter().map(|f| (&f.id, f)).collect();
    let new: BTreeMap<_, _> = new.iter().map(|f| (&f.id, f)).collect();
    let mut changes = Vec::new();
    for (id, f) in &new {
        match old.get(id) {
            None => changes.push(format!("Новый файл: {}", f.name)),
            Some(previous) if *previous != *f => changes.push(format!("Изменён: {}", f.name)),
            _ => {}
        }
    }
    for (id, f) in &old {
        if !new.contains_key(id) {
            changes.push(format!("Удалён: {}", f.name));
        }
    }
    changes
}
async fn fetch() -> Result<Vec<File>> {
    let c = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .build()?;
    let mut r = net::checked(c.get(FOLDER).send().await?).await?;
    let mut bytes = Vec::new();
    while let Some(chunk) = r.chunk().await? {
        if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
            bail!("Страница Drive превышает 4 МБ")
        }
        bytes.extend_from_slice(&chunk);
    }
    parse(std::str::from_utf8(&bytes)?)
}
pub async fn check(app: &AppHandle, rt: &Runtime) -> Result<Value> {
    let result:Result<Value>=async {
        let files=fetch().await?;
        let path=rt.config_dir.join("drive-state.json");
        let old:Vec<File>=if path.exists(){serde_json::from_slice(&std::fs::read(&path)?)?}else{baseline()};
        let changes=diff(&old,&files);
        let checked_at=SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        net::atomic(&path,&serde_json::to_vec(&files)?)?;
        Ok(json!({"status":if changes.is_empty(){"ok"}else{"changed"},"checked_at":checked_at,"changes":changes,"files":files}))
    }.await;
    match result {
        Ok(v) => {
            rt.log(
                app,
                if v["status"] == "changed" {
                    "Google Drive: обнаружены изменения сборки. Откройте папку в разделе Сборка."
                } else {
                    "Google Drive: новых файлов и изменений нет."
                },
            );
            *rt.drive.lock().unwrap() = v.clone();
            rt.changed(app);
            Ok(v)
        }
        Err(e) => {
            let message = format!("Google Drive не проверен: {e}");
            rt.log(app, &message);
            let mut state = rt.drive.lock().unwrap();
            state["status"] = json!("error");
            state["error"] = json!(message);
            drop(state);
            rt.changed(app);
            Err(e)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_escaped_listing_and_rejects_missing_or_partial() {
        let page = r"window['_DRIVE_ivd'] = '\x5b\x5b\x5b\x22abc\x22,null,\x22Сборка.zip\x22,\x22application\/zip\x22,0,null,0,0,0,123,null,null,null,456\x5d\x5d\x5d';";
        let files = parse(page).unwrap();
        assert_eq!(
            files[0],
            File {
                id: "abc".into(),
                name: "Сборка.zip".into(),
                size: 456,
                modified: 123
            }
        );
        assert!(parse("Sign in").is_err());
        assert!(parse("window['_DRIVE_ivd'] = '[null]'").is_err());
        let rows = vec![
            json!([
                "id",
                null,
                "x",
                "application/zip",
                0,
                null,
                0,
                0,
                0,
                1,
                null,
                null,
                null,
                2
            ]);
            50
        ];
        assert!(parse(&format!(
            "window['_DRIVE_ivd'] = '{}'",
            serde_json::to_string(&vec![rows]).unwrap()
        ))
        .is_err());
    }
    #[test]
    fn detects_add_edit_rename_and_delete() {
        let old = baseline();
        assert!(diff(&old, &old).is_empty());
        let mut new = old.clone();
        new[0].modified += 1;
        assert!(diff(&old, &new)[0].starts_with("Изменён"));
        new[0].name = "New.zip".into();
        assert!(diff(&old, &new)[0].contains("New.zip"));
        new.push(File {
            id: "new".into(),
            name: "config.zip".into(),
            size: 1,
            modified: 1,
        });
        assert_eq!(diff(&old, &new).len(), 2);
        assert!(diff(&old, &[])[0].starts_with("Удалён"));
    }
    #[tokio::test]
    #[ignore = "Requires public Google Drive network"]
    async fn live_public_folder() {
        let files = fetch().await.unwrap();
        assert!(!files.is_empty());
        println!("{}", serde_json::to_string(&files).unwrap());
    }
}
