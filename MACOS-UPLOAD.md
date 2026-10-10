# Upload StudHubLauncher 0.4.8 sources with GitHub update checker

The macOS workflow is configured in `.github/workflows/macos.yml`, but a build will not work until the original project files are pushed into this repository.

1. Download **StudHubLauncher-0.4.8-GitHubUpdates.zip** from the ChatGPT file/library.
2. Install [Git for Windows](https://git-scm.com/download/win) with [Git LFS](https://git-lfs.com/) and sign in to GitHub.
3. In PowerShell, from the directory containing the ZIP:

```powershell
Expand-Archive .\StudHubLauncher-0.4.8-GitHubUpdates.zip -DestinationPath .\studhub-upload -Force
git lfs install
git clone https://github.com/Agarych-ilya/StudHub_launcher.git
Copy-Item -Path .\studhub-upload\StudHubLauncher\* -Destination .\StudHub_launcher -Recurse -Force
Set-Location .\StudHub_launcher
git restore .github/workflows/macos.yml
git lfs track "src-tauri/resources/studhub-pack-1.0.2.zip"
git add .
git commit -m "Import StudHub Launcher 0.4.8 sources and modpack"
git push origin main
```

4. Go to **Actions > Build StudHub macOS DMG > Run workflow**, choose `main`.
5. After a successful run, download the `StudHubLauncher-0.4.8-macOS-universal-DMG` artifact. Tagging a version `v0.4.8` after successful validation will publish the DMG to GitHub Releases.

The macOS build is universal (x86_64 + arm64). It includes the existing 1.0.2 modpack. A free/non-notarized Apple app may trigger macOS Gatekeeper warnings. Test Minecraft + NeoForge on a real Mac before advertising it as verified. The project audit warns that the included JEI version range is incompatible with Minecraft 1.21.1.
