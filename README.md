# Bobba Launcher

Tauri launcher for **Classic AIR**, **AirPlus**, and **Bobba Client**.

**Organization:** https://github.com/Bobba-Packet  
**Repo:** https://github.com/Bobba-Packet/bobba-launcher  
**License:** [GPL-3.0](./LICENSE)

> Community project. Not affiliated with Sulake.

Install/launch flow follows [HabboCustomLauncher](https://github.com/LilithRainbows/HabboCustomLauncher) patterns (AIR shell + SWF, then `Habbo.exe -server … -ticket …`). AirPlus SWF from [HabboAirPlus](https://github.com/LilithRainbows/HabboAirPlus).

## Status (v0.1)

| Client | Install | Launch |
|---|---|---|
| Classic (official AIR) | ✅ Windows | ✅ needs login ticket |
| AirPlus | ✅ Windows | ✅ needs login ticket |
| Bobba Client | ✅ Windows | ✅ needs login ticket |

## Requirements

- Node.js 20+
- [Rust](https://rustup.rs/) (stable)
- Windows: [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) + WebView2

## Develop

```bash
npm install
npm run tauri:dev
```

1. Select **Classic** or **AirPlus**
2. Copy a Habbo login ticket (`habbo://…` or `hhus.<ticket>.V4`)
3. **Install / Update** (or just **Play** — installs if needed)
4. **Play**

## Build (local)

```bash
npm run tauri:build
```

Produces an NSIS installer under `src-tauri/target/release/bundle/nsis/`.

## CI / releases

Versioning lives on **GitHub Releases**.

| Trigger | What happens |
|---|---|
| Push / PR to `main` | Production Windows build; installer + `.exe` uploaded as workflow artifacts |
| Tag `v*` (e.g. `v0.2.0`) | Same build published as a GitHub Release with setup `.exe` (+ plain binary) |

### Ship a release

1. Bump `version` in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` (keep them in sync).
2. Commit, push to `main`, then tag and push:

```bash
git tag v0.2.0
git push origin v0.2.0
```

The **Release** workflow builds production and attaches the compiled Windows installer/binary to the release.

### Auto-updates

On launch, the app checks the [latest GitHub Release](https://github.com/Bobba-Packet/bobba-launcher/releases/latest). With **Auto-download launcher updates** enabled (default), it downloads the setup `.exe` and runs the installer.
