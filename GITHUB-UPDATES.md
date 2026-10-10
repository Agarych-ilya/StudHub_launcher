# GitHub Releases updates for StudHub Launcher

The latest source archive is `StudHubLauncher-0.4.8-GitHubUpdates.zip`, containing the GitHub update-checker implementation and its tests. The repository does not yet contain the full launcher source: upload the archive contents before attempting to build.

- At launch, and every 30 minutes, the application queries `https://api.github.com/repos/Agarych-ilya/StudHub_launcher/releases/latest`.
- It compares stable SemVer tags against the locally installed version. The app only advertises an update if the release includes a file for the platform (`.dmg` for macOS, `.exe` for Windows).
- Clicking the update notification opens **this repository's official GitHub release** in the browser. GitHub information does **not** cause any unverified program to execute.
- Existing Windows auto-install remains opt-in through the application's **Ed25519-signed update manifest and SHA-256** verification. A GitHub Release alone does not activate automatic installation. A macOS DMG is installed manually until a signed macOS updater is implemented.
- A 404 means there are no published stable releases. Network errors are nonfatal; Minecraft remains playable.

To publish a version bump, update versions in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, compile/test installers, and publish `v0.4.9` GitHub Release with the platform-specific assets.

See `MACOS-UPLOAD.md` for importing the sources, and the archive's `docs/GITHUB-UPDATES.md` for full details.
