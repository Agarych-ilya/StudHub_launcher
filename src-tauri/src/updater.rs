use anyhow::{bail, Context, Result};
use std::path::Path;

pub fn installer_name(name: &str, format: &str) -> Result<()> {
    if format != "nsis"
        || !name.ends_with(".exe")
        || !regex::Regex::new(r"^[A-Za-z0-9_.-]+$")?.is_match(name)
    {
        bail!("Автообновление Windows требует подписанного NSIS-установщика .exe")
    }
    Ok(())
}
fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
pub fn script(pid: u32, installer: &Path, current: &Path) -> Result<String> {
    let parent = current
        .parent()
        .context("Нет папки приложения")?
        .to_str()
        .context("Неверный путь приложения")?;
    if parent.contains(['\r', '\n', '"']) {
        bail!("Недопустимый путь установки")
    }
    let installer = installer.to_str().context("Неверный путь установщика")?;
    let current = current.to_str().context("Неверный путь приложения")?;
    Ok(format!("$ErrorActionPreference='Stop'\nWait-Process -Id {pid} -ErrorAction SilentlyContinue\n$p=Start-Process -FilePath {} -ArgumentList {} -PassThru -Wait\nif($p.ExitCode -eq 0){{Start-Process -FilePath {}}}else{{throw ('Ошибка установки: '+$p.ExitCode)}}\n",literal(installer),literal(&format!("/S /D={parent}")),literal(current)))
}
pub fn handoff(installer: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use base64::Engine;
        use std::os::windows::process::CommandExt;
        let current = std::env::current_exe()?;
        if current.file_name().and_then(|n| n.to_str()) != Some("StudHubLauncher.exe") {
            bail!("Установите StudHub через Setup.exe перед автообновлением")
        }
        let command = script(std::process::id(), installer, &current)?;
        let bytes: Vec<u8> = command.encode_utf16().flat_map(u16::to_le_bytes).collect();
        std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                &base64::engine::general_purpose::STANDARD.encode(bytes),
            ])
            .creation_flags(0x08000000)
            .spawn()?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        let _ = installer;
        bail!("Автоматическая установка доступна в Windows-версии")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installer_contract() {
        assert!(installer_name("StudHub-Setup.exe", "nsis").is_ok());
        for n in ["../x.exe", "x.msi", "x.exe & command"] {
            assert!(installer_name(n, "nsis").is_err())
        }
        assert!(installer_name("x.exe", "other").is_err());
    }
    #[test]
    fn waits_installs_and_restarts_without_shell_interpolation() {
        let s = script(
            123,
            Path::new("updates/a'b.exe"),
            Path::new("Some Folder/StudHubLauncher.exe"),
        )
        .unwrap();
        assert!(s.contains("Wait-Process -Id 123"));
        assert!(s.contains("'updates/a''b.exe'"));
        assert!(s.contains("/S /D=Some Folder"));
        assert!(s.contains("$p.ExitCode -eq 0"));
        assert!(s.contains("Start-Process -FilePath 'Some Folder/StudHubLauncher.exe'"));
    }
}
