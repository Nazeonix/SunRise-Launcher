//! Minecraft version manifest, downloading and launching (vanilla + Fabric / Forge / NeoForge).

use crate::{
    auth::AuthInfo,
    db::{Instance, Settings},
    gpu,
    java::{self, JavaInfo},
    loaders::{self, LoaderKind, LoaderSel},
};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::{stream, StreamExt, TryStreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fmt, fs, io,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tauri::{AppHandle, Emitter};
use tracing::{debug, info};

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const RESOURCES_URL: &str = "https://resources.download.minecraft.net";
const AUTHLIB_RELEASE: &str = "https://authlib-injector.yushi.moe/artifact/latest.json";

pub struct AuthlibConfig {
    pub jar: PathBuf,
    pub prefetched: String,
}

/// Download and verify the Authlib Injector release, and prefetch server metadata.
pub async fn authlib_config(client: &Client, root: &Path, api_root: &str) -> Result<AuthlibConfig> {
    let release: Value = client
        .get(AUTHLIB_RELEASE)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let download_url = release["download_url"]
        .as_str()
        .ok_or_else(|| anyhow!("Authlib Injector release has no download URL"))?;
    let expected = release["checksums"]["sha256"]
        .as_str()
        .ok_or_else(|| anyhow!("Authlib Injector release has no SHA-256 checksum"))?;
    let jar = root.join("authlib-injector.jar");
    let valid = if tokio::fs::try_exists(&jar).await.unwrap_or(false) {
        let bytes = tokio::fs::read(&jar).await?;
        format!("{:x}", Sha256::digest(&bytes)).eq_ignore_ascii_case(expected)
    } else {
        false
    };
    if !valid {
        let bytes = client
            .get(download_url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        if !format!("{:x}", Sha256::digest(&bytes)).eq_ignore_ascii_case(expected) {
            return Err(anyhow!(
                "Authlib Injector download failed its SHA-256 check"
            ));
        }
        tokio::fs::write(&jar, &bytes).await?;
    }
    let metadata = client
        .get(api_root)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    Ok(AuthlibConfig {
        jar,
        prefetched: STANDARD.encode(metadata),
    })
}

/// Mojang's name for the OS we run on (used by library / argument rules and natives).
fn os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

/// Classpath separator: ';' on Windows, ':' elsewhere.
fn cp_sep() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

// ───────────────────────── progress reporting ─────────────────────────

#[derive(Serialize, Clone)]
struct ProgressPayload {
    id: i64,
    text: String,
    fraction: f32,
}

/// Sends `progress` events to the UI for one instance.
#[derive(Clone)]
pub struct Reporter {
    app: AppHandle,
    id: i64,
}

impl Reporter {
    pub fn new(app: AppHandle, id: i64) -> Self {
        Self { app, id }
    }
    pub fn set(&self, text: impl Into<String>, fraction: f32) {
        let _ = self.app.emit(
            "progress",
            ProgressPayload {
                id: self.id,
                text: text.into(),
                fraction,
            },
        );
    }
}

/// No installed Java satisfies the version (the UI offers the Adoptium download).
#[derive(Debug)]
pub struct NeedJava {
    pub major: u32,
}
impl fmt::Display for NeedJava {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "This version needs Java {} or newer", self.major)
    }
}
impl std::error::Error for NeedJava {}

// ───────────────────────── version list ─────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
}

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<VersionEntry>,
}

pub async fn fetch_versions(client: &Client) -> Result<Vec<VersionEntry>> {
    let m: Manifest = client
        .get(MANIFEST_URL)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(m.versions
        .into_iter()
        .filter(|v| v.kind == "release" || v.kind == "snapshot")
        .collect())
}

/// Every instance has its own folder: saves, mods, configs, logs.
/// A custom folder (chosen by the user) wins over the default `<data>/instances/<id>`.
pub fn game_dir(root: &Path, inst: &Instance) -> PathBuf {
    if inst.dir.trim().is_empty() {
        root.join("instances").join(inst.id.to_string())
    } else {
        PathBuf::from(inst.dir.trim())
    }
}

// ───────────────────────── downloading ─────────────────────────

pub async fn download(client: &Client, url: &str, dest: &Path) -> Result<()> {
    if tokio::fs::try_exists(dest).await.unwrap_or(false) {
        return Ok(());
    }
    if let Some(p) = dest.parent() {
        tokio::fs::create_dir_all(p).await?;
    }
    let bytes = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let tmp = dest.with_extension("part");
    tokio::fs::write(&tmp, &bytes).await?;
    tokio::fs::rename(&tmp, dest).await?;
    Ok(())
}

/// Download many files, 16 at a time.
async fn download_many(
    client: &Client,
    jobs: Vec<(String, PathBuf)>,
    rep: &Reporter,
    label: &str,
    from: f32,
    to: f32,
) -> Result<()> {
    let total = jobs.len().max(1);
    let done = Arc::new(AtomicUsize::new(0));
    debug!(label, count = jobs.len(), "downloading");

    stream::iter(jobs.into_iter().map(|(url, path)| {
        let client = client.clone();
        let done = done.clone();
        let rep = rep.clone();
        let label = label.to_string();
        async move {
            let mut res = download(&client, &url, &path).await;
            if res.is_err() {
                res = download(&client, &url, &path).await; // one retry
            }
            res.map_err(|e| anyhow!("{url}: {e}"))?;
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            if d % 8 == 0 || d == total {
                rep.set(
                    format!("{label} ({d}/{total})"),
                    from + (to - from) * d as f32 / total as f32,
                );
            }
            Ok::<(), anyhow::Error>(())
        }
    }))
    .buffer_unordered(16)
    .try_collect::<Vec<()>>()
    .await?;
    Ok(())
}

// ───────────────────────── rules ─────────────────────────

fn os_matches(os: &Value) -> bool {
    if let Some(name) = os.get("name").and_then(Value::as_str) {
        if name != os_name() {
            return false;
        }
    }
    if let Some(arch) = os.get("arch").and_then(Value::as_str) {
        let cur = std::env::consts::ARCH;
        let ok = match arch {
            "x86" => cur == "x86",
            "arm64" => cur == "aarch64",
            _ => true,
        };
        if !ok {
            return false;
        }
    }
    true
}

/// Mojang "rules" evaluation for Linux, ignoring feature-gated rules (demo mode, custom resolution…).
fn rules_allow(rules: Option<&Value>) -> bool {
    let Some(arr) = rules.and_then(Value::as_array) else {
        return true;
    };
    let mut allowed = false;
    for r in arr {
        if r.get("features").is_some() {
            continue;
        }
        let matches = r.get("os").map(os_matches).unwrap_or(true);
        if matches {
            allowed = r["action"] == "allow";
        }
    }
    allowed
}

/// "group:artifact:version[:classifier][@ext]" -> "group/artifact/version/artifact-version[-classifier].ext"
fn maven_path(name: &str) -> Option<String> {
    let (coords, ext) = match name.split_once('@') {
        Some((c, e)) => (c, e),
        None => (name, "jar"),
    };
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let (artifact, version) = (parts[1], parts[2]);
    let file = if parts.len() >= 4 {
        format!("{artifact}-{version}-{}.{ext}", parts[3])
    } else {
        format!("{artifact}-{version}.{ext}")
    };
    Some(format!("{group}/{artifact}/{version}/{file}"))
}

// ───────────────────────── Java selection ─────────────────────────

/// `pref` = chosen java path ("" = auto).
pub fn pick_java(javas: &[JavaInfo], pref: &str, need: u32) -> Result<JavaInfo> {
    if !pref.is_empty() {
        if let Some(j) = javas.iter().find(|j| j.path == pref) {
            return if j.major >= need {
                Ok(j.clone())
            } else {
                Err(anyhow!("Selected Java {} is too old – this version needs Java {}+ (change it in the instance settings)", j.major, need))
            };
        }
    }
    match java::pick(javas, need) {
        Some(j) => Ok(j.clone()),
        None => Err(anyhow::Error::new(NeedJava { major: need })),
    }
}

// ───────────────────────── install ─────────────────────────

pub struct Installed {
    /// vanilla Minecraft version id (also `${version_name}`)
    pub id: String,
    /// e.g. "1.20.1 + Forge 47.2.0"
    pub display: String,
    pub kind: String,
    pub meta: Value,
    pub jar: PathBuf,
    pub classpath: Vec<PathBuf>,
    pub natives: PathBuf,
    pub asset_index: String,
    pub main_class: String,
    pub java: JavaInfo,
}

fn extract_natives(jar: &Path, dest: &Path) -> Result<()> {
    let mut z = zip::ZipArchive::new(fs::File::open(jar)?)?;
    fs::create_dir_all(dest)?;
    for i in 0..z.len() {
        let mut e = z.by_index(i)?;
        if e.is_dir() || e.name().starts_with("META-INF") {
            continue;
        }
        let Some(name) = e
            .enclosed_name()
            .and_then(|p| p.file_name().map(|f| f.to_owned()))
        else {
            continue;
        };
        let mut out = fs::File::create(dest.join(name))?;
        io::copy(&mut e, &mut out)?;
    }
    Ok(())
}

pub async fn install(
    client: &Client,
    entry: &VersionEntry,
    loader: &LoaderSel,
    root: &Path,
    rep: &Reporter,
    javas: &[JavaInfo],
    java_pref: &str,
) -> Result<Installed> {
    rep.set(format!("Fetching {} metadata…", entry.id), 0.02);
    let vdir = root.join("versions").join(&entry.id);
    let meta_path = vdir.join(format!("{}.json", entry.id));
    download(client, &entry.url, &meta_path).await?;
    let vanilla: Value = serde_json::from_slice(&tokio::fs::read(&meta_path).await?)?;

    // client jar
    rep.set("Downloading client…", 0.04);
    let jar = vdir.join(format!("{}.jar", entry.id));
    let jar_url = vanilla["downloads"]["client"]["url"]
        .as_str()
        .ok_or_else(|| anyhow!("version has no client download"))?;
    download(client, jar_url, &jar).await?;

    // Java (also needed by the Forge / NeoForge installers)
    let need = vanilla["javaVersion"]["majorVersion"].as_u64().unwrap_or(8) as u32;
    let java = pick_java(javas, java_pref, need)?;
    info!(java = %java.path, major = java.major, "using Java");

    // mod loader profile merged over vanilla
    let meta = if loader.kind == LoaderKind::Vanilla {
        vanilla
    } else {
        let prof = loaders::profile(client, loader, &entry.id, root, &java, rep).await?;
        loaders::merge(&vanilla, &prof)
    };

    // libraries
    let libs_dir = root.join("libraries");
    let natives = vdir.join("natives");
    let mut jobs: Vec<(String, PathBuf)> = Vec::new();
    let mut classpath: Vec<PathBuf> = Vec::new();
    let mut native_jars: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for lib in meta["libraries"].as_array().into_iter().flatten() {
        if !rules_allow(lib.get("rules")) {
            continue;
        }
        let name = lib["name"].as_str().unwrap_or("");
        let (rel, url): (String, String) = if let Some(a) = lib["downloads"].get("artifact") {
            match a["path"]
                .as_str()
                .map(String::from)
                .or_else(|| maven_path(name))
            {
                Some(r) => (r, a["url"].as_str().unwrap_or("").to_string()),
                None => continue,
            }
        } else if lib.get("downloads").is_none() {
            // Fabric-style entry: just a maven name and a repository URL
            match maven_path(name) {
                Some(r) => {
                    let base = lib["url"]
                        .as_str()
                        .unwrap_or("https://libraries.minecraft.net/");
                    let base = if base.ends_with('/') {
                        base.to_string()
                    } else {
                        format!("{base}/")
                    };
                    let u = format!("{base}{r}");
                    (r, u)
                }
                None => continue,
            }
        } else {
            (String::new(), String::new()) // natives-only entry
        };

        if !rel.is_empty() && seen.insert(rel.clone()) {
            let path = libs_dir.join(&rel);
            if !url.is_empty() {
                jobs.push((url, path.clone()));
            }
            // empty url = generated by the Forge/NeoForge installer, already on disk
            classpath.push(path);
        }

        // legacy (<= 1.18) natives classifiers
        if let Some(key) = lib["natives"][os_name()].as_str() {
            let key = key.replace("${arch}", "64");
            if let Some(c) = lib["downloads"]["classifiers"].get(&key) {
                if let (Some(p), Some(u)) = (c["path"].as_str(), c["url"].as_str()) {
                    let path = libs_dir.join(p);
                    jobs.push((u.to_string(), path.clone()));
                    native_jars.push(path);
                }
            }
        }
    }
    download_many(client, jobs, rep, "Libraries", 0.10, 0.30).await?;

    let natives2 = natives.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        for nj in &native_jars {
            extract_natives(nj, &natives2)?;
        }
        fs::create_dir_all(&natives2)?;
        Ok(())
    })
    .await??;

    // assets
    rep.set("Fetching asset index…", 0.30);
    let asset_index = meta["assetIndex"]["id"]
        .as_str()
        .ok_or_else(|| anyhow!("missing assetIndex"))?
        .to_string();
    let idx_url = meta["assetIndex"]["url"]
        .as_str()
        .ok_or_else(|| anyhow!("missing assetIndex url"))?;
    let idx_path = root
        .join("assets/indexes")
        .join(format!("{asset_index}.json"));
    download(client, idx_url, &idx_path).await?;
    let idx: Value = serde_json::from_slice(&tokio::fs::read(&idx_path).await?)?;

    let mut seen_hash = HashSet::new();
    let mut jobs = Vec::new();
    if let Some(objs) = idx["objects"].as_object() {
        for (_, o) in objs {
            if let Some(h) = o["hash"].as_str() {
                if h.len() > 2 && seen_hash.insert(h.to_string()) {
                    let p = &h[..2];
                    jobs.push((
                        format!("{RESOURCES_URL}/{p}/{h}"),
                        root.join("assets/objects").join(p).join(h),
                    ));
                }
            }
        }
    }
    download_many(client, jobs, rep, "Assets", 0.30, 0.98).await?;

    let display = if loader.kind == LoaderKind::Vanilla {
        entry.id.clone()
    } else {
        format!("{} + {} {}", entry.id, loader.kind.label(), loader.version)
    };

    Ok(Installed {
        id: entry.id.clone(),
        display,
        kind: entry.kind.clone(),
        jar,
        classpath,
        natives,
        asset_index,
        main_class: meta["mainClass"]
            .as_str()
            .unwrap_or("net.minecraft.client.main.Main")
            .to_string(),
        java,
        meta,
    })
}

// ───────────────────────── launch ─────────────────────────

fn subst(s: &str, map: &HashMap<&str, String>) -> String {
    let mut out = s.to_string();
    for (k, v) in map {
        out = out.replace(&format!("${{{k}}}"), v);
    }
    out
}

fn process_args(v: Option<&Value>, map: &HashMap<&str, String>) -> Vec<String> {
    let mut out = Vec::new();
    for item in v.and_then(Value::as_array).into_iter().flatten() {
        match item {
            Value::String(s) => out.push(subst(s, map)),
            Value::Object(_) => {
                if !rules_allow(item.get("rules")) {
                    continue;
                }
                match &item["value"] {
                    Value::String(s) => out.push(subst(s, map)),
                    Value::Array(a) => {
                        for x in a.iter().filter_map(Value::as_str) {
                            out.push(subst(x, map));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

pub fn launch(
    ins: &Installed,
    inst: &Instance,
    settings: &Settings,
    root: &Path,
    auth: &AuthInfo,
    authlib: Option<&AuthlibConfig>,
) -> Result<tokio::process::Child> {
    let game_dir = game_dir(root, inst);
    fs::create_dir_all(&game_dir)?;

    let mut cp: Vec<String> = ins
        .classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    cp.push(ins.jar.to_string_lossy().to_string());
    let classpath = cp.join(cp_sep());

    let mut map: HashMap<&str, String> = HashMap::new();
    map.insert("auth_player_name", auth.name.clone());
    map.insert("version_name", ins.id.clone());
    map.insert("game_directory", game_dir.to_string_lossy().to_string());
    map.insert(
        "assets_root",
        root.join("assets").to_string_lossy().to_string(),
    );
    map.insert(
        "game_assets",
        root.join("assets").to_string_lossy().to_string(),
    );
    map.insert("assets_index_name", ins.asset_index.clone());
    map.insert("auth_uuid", auth.uuid.clone());
    map.insert("auth_access_token", auth.token.clone());
    map.insert("auth_session", "0".into());
    map.insert("clientid", String::new());
    map.insert("auth_xuid", String::new());
    map.insert("user_type", auth.user_type.clone());
    map.insert("user_properties", "{}".into());
    map.insert("version_type", ins.kind.clone());
    map.insert(
        "natives_directory",
        ins.natives.to_string_lossy().to_string(),
    );
    map.insert("launcher_name", "Sunrise".into());
    map.insert("launcher_version", env!("CARGO_PKG_VERSION").into());
    map.insert("classpath", classpath.clone());
    // used by Forge / NeoForge
    map.insert(
        "library_directory",
        root.join("libraries").to_string_lossy().to_string(),
    );
    map.insert("classpath_separator", cp_sep().into());

    let (jvm_args, game_args): (Vec<String>, Vec<String>) = if ins.meta.get("arguments").is_some() {
        // modern format (1.13+)
        (
            process_args(ins.meta["arguments"].get("jvm"), &map),
            process_args(ins.meta["arguments"].get("game"), &map),
        )
    } else {
        // legacy format
        (
            vec![
                format!("-Djava.library.path={}", ins.natives.display()),
                "-cp".into(),
                classpath,
            ],
            ins.meta["minecraftArguments"]
                .as_str()
                .unwrap_or("")
                .split_whitespace()
                .map(|s| subst(s, &map))
                .collect(),
        )
    };

    let ram = if inst.ram_mb > 0 {
        inst.ram_mb
    } else {
        settings.ram_mb
    };
    let mut cmd = tokio::process::Command::new(&ins.java.path);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console window next to the game
    cmd.arg(format!("-Xmx{ram}M")).arg("-Xms512M");
    cmd.args(&jvm_args);
    if let Some(config) = authlib {
        let api_root = auth
            .api_root
            .as_deref()
            .ok_or_else(|| anyhow!("Missing external authentication server"))?;
        cmd.arg(format!("-javaagent:{}={api_root}", config.jar.display()));
        cmd.arg(format!(
            "-Dauthlibinjector.yggdrasil.prefetched={}",
            config.prefetched
        ));
    }
    cmd.arg(&ins.main_class);
    cmd.args(&game_args);
    cmd.current_dir(&game_dir);

    if settings.use_discrete_gpu {
        for (k, v) in gpu::offload_env(&gpu::detect()) {
            cmd.env(k, v);
        }
    }

    let log = fs::File::create(game_dir.join("sunrise-latest.log"))?;
    cmd.stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    info!(instance = inst.id, version = %ins.display, java = %ins.java.path, "launching Minecraft");
    Ok(cmd.spawn()?)
}
