//! Read-only GitHub Releases version discovery. No unsigned installer is ever executed.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::time::Duration;

pub const RELEASES_URL: &str = "https://github.com/Agarych-ilya/StudHub_launcher/releases";
const LATEST_API: &str = "https://api.github.com/repos/Agarych-ilya/StudHub_launcher/releases/latest";
const RELEASE_TAG_PREFIX: &str = "https://github.com/Agarych-ilya/StudHub_launcher/releases/tag/";

/// Only allow release links inside the official project. Never open arbitrary URLs from the API.
pub fn validated_release_url(input: &str) -> Result<&str> {
    if !input.starts_with(RELEASE_TAG_PREFIX) || input.len() > 400 {
        bail!("Недопустимая ссылка на релиз GitHub");
    }
    let tag = &input[RELEASE_TAG_PREFIX.len()..];
    if tag.is_empty() || !tag.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_')) {
        bail!("Недопустимая ссылка на релиз GitHub");
    }
    Ok(input)
}

pub fn interpret_release(value: &Value, current_version: &str, extension: &str) -> Result<Value> {
    let raw = value["tag_name"].as_str().context("У релиза нет номера версии")?;
    let version = raw.strip_prefix('v').unwrap_or(raw);
    let latest = semver::Version::parse(version).context("Неверная версия релиза")?;
    let current = semver::Version::parse(current_version)?;
    if value["draft"].as_bool().unwrap_or(false) || value["prerelease"].as_bool().unwrap_or(false) {
        bail!("Предварительный выпуск не является стабильным релизом");
    }
    let release_url = validated_release_url(value["html_url"].as_str().context("Нет ссылки на GitHub Release")?)?;
    if !release_url.ends_with(&format!("/{raw}")) {
        bail!("Номер версии не соответствует ссылке GitHub");
    }
    let has_platform_installer = value["assets"].as_array().is_some_and(|assets| assets.iter().any(|asset| {
        let name = asset["name"].as_str().unwrap_or("").to_ascii_lowercase();
        name.ends_with(extension)
    }));
    Ok(json!({
        "status": if latest <= current {"current"} else if has_platform_installer {"available"} else {"pending_asset"},
        "version": version,
        "release_url": release_url,
        "notes": value["body"].as_str().unwrap_or("").chars().take(2000).collect::<String>(),
        "published_at": value["published_at"].as_str().unwrap_or("")
    }))
}

/// HTTP errors don't block Minecraft. 404 means no published release yet.
pub async fn check(current_version: &str) -> Result<Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(9))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client.get(LATEST_API)
        .header(reqwest::header::USER_AGENT, format!("StudHubLauncher/{current_version}"))
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(json!({"status":"no_releases"}));
    }
    if !response.status().is_success() {
        bail!("GitHub API ответил HTTP {}", response.status());
    }
    if response.content_length().is_some_and(|n| n > 128 * 1024) {
        bail!("Слишком большой ответ GitHub API");
    }
    let bytes = response.bytes().await?;
    if bytes.len() > 128 * 1024 {
        bail!("Слишком большой ответ GitHub API");
    }
    let extension = if cfg!(windows) { ".exe" } else if cfg!(target_os = "macos") { ".dmg" } else { ".appimage" };
    interpret_release(&serde_json::from_slice::<Value>(&bytes)?, current_version, extension)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn semver_handles_newer_equal_and_older() {
        let release = json!({"tag_name":"v0.4.9","html_url":"https://github.com/Agarych-ilya/StudHub_launcher/releases/tag/v0.4.9","draft":false,"prerelease":false,"body":"Улучшения интерфейса","assets":[{"name":"StudHubLauncher-0.4.9-macOS.dmg"}]});
        assert_eq!(interpret_release(&release, "0.4.8", ".dmg").unwrap()["status"], "available");
        assert_eq!(interpret_release(&release, "0.4.8", ".exe").unwrap()["status"], "pending_asset");
        assert_eq!(interpret_release(&release, "0.4.9", ".dmg").unwrap()["status"], "current");
        assert_eq!(interpret_release(&release, "0.5.0", ".dmg").unwrap()["status"], "current");
    }
    #[test]
    fn rejects_untrusted_release_urls() {
        for url in ["http://github.com/Agarych-ilya/StudHub_launcher/releases/tag/v1.0.0", "https://evil.com/", "https://github.com/Agarych-ilya/StudHub_launcher/releases/tag/v1.0.0?redirect=https://evil.com", "https://github.com/Agarych-ilya/StudHub_launcher/releases/tag/"] {
            assert!(validated_release_url(url).is_err());
        }
    }
    #[test]
    fn invalid_or_draft_releases_are_not_shown() {
        let release = json!({"tag_name":"v0.4.9","html_url":"https://github.com/Agarych-ilya/StudHub_launcher/releases/tag/v0.4.9","draft":true});
        assert!(interpret_release(&release, "0.4.8", ".dmg").is_err());
    }
}
