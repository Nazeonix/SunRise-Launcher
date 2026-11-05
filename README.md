# ☀ Sunrise (Tauri 2 edition)

A simple Minecraft launcher with an **instances sidebar**.

| Layer          | Technology                                   |
|----------------|----------------------------------------------|
| Language       | Rust                                         |
| Desktop        | Tauri 2                                      |
| Frontend       | Svelte 5 (runes)                             |
| Styling        | plain CSS (Material 3 Expressive-style)      |
| Database       | SQLite (`rusqlite`, bundled)                 |
| Async          | Tokio                                        |
| Serialization  | Serde                                        |
| Authentication | OAuth 2.0 + PKCE (loopback redirect)         |
| Logging        | `tracing`                                    |

## Features
- **Instances** – every instance has its own Minecraft version, mod loader, Java, memory, colour,
  mods, worlds and logs (`~/.local/share/Sunrise/instances/<id>/`). Sidebar with search, running indicator, “New instance”.
- **Vanilla, Fabric, Forge, NeoForge** (the launcher defaults to each loader's recommended/stable build for the selected Minecraft version; exact builds can be pinned).
- Material-styled **Home** launch panel with instance/profile selection and quick links; instance tabs include **Overview · Mods** (enable/disable/delete and safe Modrinth updates) **· Worlds** (browse, rename, back up, open) **· Resource Packs · Logs · Settings**.
- Saved launch profiles with alternate per-instance Java and memory settings.
- Java detection (25 / 21 / 17 / 11 / 8 …) with per-instance override, manual paths and Adoptium download links.
- Linux **dedicated-GPU (PRIME) offload** for the game process.
- 8 Material You colour schemes, procedural pixel-art banner, wavy progress, cookie/clover shapes.
- Ely.by and LittleSkin accounts, plus offline mode. Microsoft sign-in is currently disabled.

## Versioning

The current release is **0.3.0**. Bump bug-fix releases by **0.1.0** and minor releases by **0.0.5**; keep the package, Tauri config, and Cargo versions in sync.

## Build & run

```bash
# Arch Linux prerequisites (see https://v2.tauri.app/start/prerequisites/)
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module \
                        libappindicator-gtk3 librsvg xdotool nodejs npm rustup
rustup default stable

npm install
npm run tauri dev       # development (hot reload)
npm run tauri build     # release bundle
```

If the window stays blank on NVIDIA/Wayland, try `WEBKIT_DISABLE_DMABUF_RENDERER=1 npm run tauri dev`.
Log level: `RUST_LOG=debug npm run tauri dev`.

Instances are stored in `<data>/instances/<id>` by default. You can pick another folder in **Settings → Storage**
(default for new instances) or per instance in the **New instance** dialog. Existing instances stay where they are.

Data: `~/.local/share/Sunrise/` (`sunrise.db`, `versions/`, `libraries/`, `assets/`, `instances/`, `installers/`).

## Windows

Prerequisites (one time):
1. **Microsoft C++ Build Tools** – https://visualstudio.microsoft.com/visual-cpp-build-tools/ → workload *Desktop development with C++*
2. **WebView2 runtime** – already included in Windows 11 and current Windows 10
3. **Rust** – https://rustup.rs (default `x86_64-pc-windows-msvc` toolchain)
4. **Node.js LTS** – https://nodejs.org

```powershell
npm install
npm run tauri dev       # development
npm run tauri build     # installers in src-tauri\target\release\bundle\  (nsis .exe and msi)
```

- Data lives in `%APPDATA%\sunrise\Sunrise\data`.
- Java is detected in `JAVA_HOME`, `PATH` and the usual install folders
  (`Program Files\Eclipse Adoptium`, `Java`, `Microsoft`, `Zulu`, `Amazon Corretto`, …).
- The “dedicated GPU” option is Linux-only; on Windows choose the GPU for `javaw.exe`/`java.exe`
  under *Settings → System → Display → Graphics*.
- Unsigned installers trigger a SmartScreen warning.
- A GitHub Actions workflow (`.github/workflows/build.yml`) builds Windows and Linux bundles for you.

Microsoft sign-in is currently disabled. Use an offline profile or an Ely.by/LittleSkin account.

<!-- Microsoft sign-in setup retained while the provider is disabled:
## Microsoft sign-in (OAuth 2.0 + PKCE)
Sunrise opens your browser at Microsoft's login page, listens on `http://localhost:<random port>` for the
redirect, and exchanges the code with a PKCE verifier (no client secret). Then Xbox Live → XSTS → Minecraft services.
Only the refresh token is stored (in `sunrise.db`, file mode 600).

You need **your own Azure app registration** (free) – Microsoft always requires a client ID:

1. https://portal.azure.com → *Microsoft Entra ID* → *App registrations* → **New registration**
   - Supported account types: **Personal Microsoft accounts only** (or “any … and personal”)
   - Redirect URI: platform **Mobile and desktop applications** → `http://localhost`
2. *Authentication* → **Allow public client flows = Yes**
3. Copy the **Application (client) ID** into **Settings → Microsoft sign-in** (or set `SUNRISE_MS_CLIENT_ID`)
4. New apps are blocked by Minecraft services (`403 Invalid app registration`) until Mojang approves them:
   https://aka.ms/mce-reviewappid

Until then the launcher works in **offline mode** (username under *Account*).
-->

## Ely.by and LittleSkin sign-in

Open **Account → Add another account**, choose Ely.by or LittleSkin, then enter your account name/email and password.
For Ely.by accounts with two-factor authentication, enter the current code too. Sunrise uses the provider's
Yggdrasil API and saves the returned access token; your password is used only for sign-in. When launching,
Sunrise downloads and SHA-256 verifies Authlib Injector, then configures Minecraft to use that provider's
authentication and session services. This lets the account authenticate on servers configured for the same
provider; it does not make these accounts interchangeable with Microsoft/Mojang accounts.

Saved accounts can be switched or removed from **Account**. The active account is used for new game launches.

## Mod loaders
- **Fabric** – profile from `meta.fabricmc.net` merged over vanilla. Most mods need *Fabric API* in `mods/`.
- **Forge / NeoForge** – installer downloaded and run headless (`--installClient`), first launch takes a minute;
  logs in `installers/`. NeoForge supports Minecraft 1.20.2+.

## Modpacks (Modrinth)

**Sidebar → Modpacks.** Search, pick a version, press *Install*: Sunrise creates a new instance, downloads the pack's
files, extracts the overrides and sets the Minecraft version and loader. Press *Play* afterwards to download the game.
**Import file…** installs a local `.mrpack`.

- **Modrinth** works with no setup (`.mrpack`, files checked against their SHA-512).
- CurseForge browsing and imports are currently disabled.
- Quilt packs are not supported (Fabric, Forge and NeoForge are).

## App icon
`public/icon.svg` is the scalable source. Regenerate the platform PNG/ICO files in `src-tauri/icons/` with
`npm run tauri icon public/icon.svg -o src-tauri/icons` after changing it.

## Project layout
```
src/                 Svelte 5 UI  (App, lib/Sidebar, InstanceView, dialogs, state.svelte.js, themes.js)
src-tauri/src/
  lib.rs             app setup, tracing, state
  commands.rs        Tauri commands + play task
  db.rs              SQLite (instances, settings, account)
  launcher.rs        manifest, downloads (Tokio), launch
  loaders.rs         Fabric / Forge / NeoForge
  auth.rs            Microsoft OAuth and Yggdrasil authentication
  java.rs, gpu.rs    Java + GPU detection
```
