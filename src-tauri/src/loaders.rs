//! Mod loaders: Fabric (meta API) and Forge / NeoForge (official installers).
//!
//! Every loader produces a "version profile" JSON with `inheritsFrom: <vanilla id>`.
//! We merge it on top of the vanilla JSON (libraries, arguments, main class) and launch that.

use crate::{
    java::JavaInfo,
    launcher::{self, Reporter},
};
use anyhow::{anyhow, bail, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::{cmp::Ordering, collections::HashSet, path::Path, process::Stdio};
use tokio::process::Command;

const FABRIC_META: &str = "https://meta.fabricmc.net/v2/versions/loader";
const FORGE_META: &str =
    "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
const FORGE_PROMOS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEO_META: &str =
    "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoaderKind {
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
}

impl LoaderKind {
    pub fn label(self) -> &'static str {
        match self {
            LoaderKind::Vanilla => "Vanilla",
            LoaderKind::Fabric => "Fabric",
            LoaderKind::Forge => "Forge",
            LoaderKind::NeoForge => "NeoForge",
        }
    }
    pub fn from_key(k: &str) -> Self {
        match k {
            "fabric" => LoaderKind::Fabric,
            "forge" => LoaderKind::Forge,
            "neoforge" => LoaderKind::NeoForge,
            _ => LoaderKind::Vanilla,
        }
    }
}

/// A loader + a concrete loader version (empty for vanilla).
#[derive(Clone)]
pub struct LoaderSel {
    pub kind: LoaderKind,
    pub version: String,
}

// ───────────────────────── version lists ─────────────────────────

fn xml_versions(txt: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = txt;
    while let Some(i) = rest.find("<version>") {
        let after = &rest[i + 9..];
        match after.find("</version>") {
            Some(j) => {
                out.push(after[..j].trim().to_string());
                rest = &after[j + 10..];
            }
            None => break,
        }
    }
    out
}

/// NeoForge versions are "<mc minor>.<mc patch>.<build>": 1.21.1 -> 21.1.x, 1.21 -> 21.0.x
fn neoforge_prefix(mc: &str) -> Option<String> {
    if let Some(rest) = mc.strip_prefix("1.") {
        let mut it = rest.split('.');
        let major: u32 = it.next()?.parse().ok()?;
        let minor: u32 = it.next().and_then(|m| m.parse().ok()).unwrap_or(0);
        if major < 20 || (major == 20 && minor < 2) {
            return None;
        }
        Some(format!("{major}.{minor}."))
    } else if mc.chars().next()?.is_ascii_digit() && !mc.contains('-') && !mc.contains('w') {
        Some(format!("{mc}.")) // year-based versions (best guess)
    } else {
        None
    }
}

/// Loader versions available for a Minecraft version, recommended build first.
pub async fn versions(client: &Client, kind: LoaderKind, mc: &str) -> Result<Vec<String>> {
    let out: Vec<String> = match kind {
        LoaderKind::Vanilla => Vec::new(),
        LoaderKind::Fabric => {
            let v: Value = client
                .get(format!("{FABRIC_META}/{mc}"))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let arr = v
                .as_array()
                .ok_or_else(|| anyhow!("unexpected Fabric response"))?;
            let mut stable = Vec::new();
            let mut other = Vec::new();
            for e in arr {
                if let Some(ver) = e["loader"]["version"].as_str() {
                    if e["loader"]["stable"].as_bool().unwrap_or(false) {
                        stable.push(ver.to_string());
                    } else {
                        other.push(ver.to_string());
                    }
                }
            }
            stable.extend(other);
            stable
        }
        LoaderKind::Forge => {
            let txt = client
                .get(FORGE_META)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            let prefix = format!("{mc}-");
            let mut v: Vec<String> = xml_versions(&txt)
                .into_iter()
                .filter_map(|x| x.strip_prefix(&prefix).map(String::from))
                .collect();
            v.sort_by(|a, b| version_order(b, a));
            if let Ok(promotions) = client
                .get(FORGE_PROMOS)
                .send()
                .await
                .and_then(|r| r.error_for_status())
            {
                if let Ok(data) = promotions.json::<Value>().await {
                    let key = format!("{mc}-recommended");
                    if let Some(recommended) = data["promos"].get(key).and_then(Value::as_str) {
                        if let Some(pos) = v.iter().position(|version| version == recommended) {
                            let version = v.remove(pos);
                            v.insert(0, version);
                        }
                    }
                }
            }
            v
        }
        LoaderKind::NeoForge => {
            let prefix = neoforge_prefix(mc)
                .ok_or_else(|| anyhow!("NeoForge supports Minecraft 1.20.2 and newer"))?;
            let txt = client
                .get(NEO_META)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            let all: Vec<String> = xml_versions(&txt)
                .into_iter()
                .filter(|x| x.starts_with(&prefix))
                .collect();
            let (mut stable, mut beta): (Vec<String>, Vec<String>) =
                all.into_iter().partition(|x| !x.contains('-'));
            stable.sort_by(|a, b| version_order(b, a));
            beta.sort_by(|a, b| version_order(b, a));
            stable.extend(beta);
            stable
        }
    };
    if out.is_empty() && kind != LoaderKind::Vanilla {
        bail!("{} has no build for Minecraft {mc}", kind.label());
    }
    Ok(out)
}

/// The requested loader version, or the recommended build when `requested` is empty.
pub async fn resolve(
    client: &Client,
    kind: LoaderKind,
    mc: &str,
    requested: &str,
) -> Result<LoaderSel> {
    if kind == LoaderKind::Vanilla {
        return Ok(LoaderSel {
            kind,
            version: String::new(),
        });
    }
    if !requested.is_empty() {
        return Ok(LoaderSel {
            kind,
            version: requested.to_string(),
        });
    }
    let list = versions(client, kind, mc).await?;
    Ok(LoaderSel {
        kind,
        version: list
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no loader versions"))?,
    })
}

/// Compare numeric version components so builds like 21.1.100 sort after 21.1.99.
fn version_order(a: &str, b: &str) -> Ordering {
    let a: Vec<u64> = a
        .split(|c: char| c == '.' || c == '+' || c == '-')
        .filter_map(|part| part.parse().ok())
        .collect();
    let b: Vec<u64> = b
        .split(|c: char| c == '.' || c == '+' || c == '-')
        .filter_map(|part| part.parse().ok())
        .collect();
    for (left, right) in a.iter().zip(&b) {
        match left.cmp(right) {
            Ordering::Equal => continue,
            order => return order,
        }
    }
    a.len().cmp(&b.len())
}

// ───────────────────────── installing ─────────────────────────

/// The loader's version-profile JSON (downloaded / installed on demand).
pub async fn profile(
    client: &Client,
    sel: &LoaderSel,
    mc: &str,
    root: &Path,
    java: &JavaInfo,
    rep: &Reporter,
) -> Result<Value> {
    match sel.kind {
        LoaderKind::Vanilla => bail!("vanilla has no loader profile"),
        LoaderKind::Fabric => fabric_profile(client, &sel.version, mc, root).await,
        LoaderKind::Forge | LoaderKind::NeoForge => {
            installer_profile(client, sel, mc, root, java, rep).await
        }
    }
}

async fn fabric_profile(client: &Client, ver: &str, mc: &str, root: &Path) -> Result<Value> {
    let id = format!("fabric-loader-{ver}-{mc}");
    let path = root.join("versions").join(&id).join(format!("{id}.json"));
    launcher::download(
        client,
        &format!("{FABRIC_META}/{mc}/{ver}/profile/json"),
        &path,
    )
    .await?;
    Ok(serde_json::from_slice(&tokio::fs::read(&path).await?)?)
}

/// Looks for a profile the installer already created.
fn find_installed(root: &Path, mc: &str, ver: &str, kind: LoaderKind) -> Option<Value> {
    let rd = std::fs::read_dir(root.join("versions")).ok()?;
    for e in rd.flatten() {
        let id = e.file_name().to_string_lossy().to_string();
        let lid = id.to_lowercase();
        let is_kind = match kind {
            LoaderKind::NeoForge => lid.contains("neoforge"),
            LoaderKind::Forge => lid.contains("forge") && !lid.contains("neoforge"),
            _ => false,
        };
        if !is_kind || !id.contains(ver) {
            continue;
        }
        if let Ok(bytes) = std::fs::read(e.path().join(format!("{id}.json"))) {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                if v["inheritsFrom"].as_str() == Some(mc) || v.get("inheritsFrom").is_none() {
                    return Some(v);
                }
            }
        }
    }
    None
}

async fn installer_profile(
    client: &Client,
    sel: &LoaderSel,
    mc: &str,
    root: &Path,
    java: &JavaInfo,
    rep: &Reporter,
) -> Result<Value> {
    let ver = sel.version.as_str();
    if let Some(v) = find_installed(root, mc, ver, sel.kind) {
        return Ok(v);
    }
    let (name, url) = match sel.kind {
        LoaderKind::Forge => (
            "forge",
            format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{ver}/forge-{mc}-{ver}-installer.jar"),
        ),
        _ => (
            "neoforge",
            format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{ver}/neoforge-{ver}-installer.jar"),
        ),
    };
    let label = sel.kind.label();
    let dir = root.join("installers");
    tokio::fs::create_dir_all(&dir).await?;
    let jar = dir.join(format!("{name}-{mc}-{ver}-installer.jar"));

    rep.set(format!("Downloading {label} installer…"), 0.02);
    launcher::download(client, &url, &jar).await?;

    // the installers refuse to run without a launcher profile file
    let lp = root.join("launcher_profiles.json");
    if !lp.exists() {
        tokio::fs::write(&lp, r#"{"profiles":{},"version":3}"#).await?;
    }

    rep.set(
        format!("Installing {label} {ver} (this can take a minute)…"),
        0.03,
    );
    let log_path = dir.join(format!("{name}-install.log"));
    let log = std::fs::File::create(&log_path)?;
    let mut cmd = Command::new(&java.path);
    cmd.arg("-jar")
        .arg(&jar)
        .arg("--installClient")
        .arg(root)
        .current_dir(root)
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let status = cmd.status().await?;

    match find_installed(root, mc, ver, sel.kind) {
        Some(v) if status.success() => Ok(v),
        _ => {
            let tail = std::fs::read_to_string(&log_path).unwrap_or_default();
            let lines: Vec<&str> = tail.lines().collect();
            let last = lines[lines.len().saturating_sub(5)..].join(" | ");
            bail!("{label} installer failed: {last}")
        }
    }
}

// ───────────────────────── merging ─────────────────────────

fn lib_key(lib: &Value) -> String {
    let name = lib["name"].as_str().unwrap_or("");
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() >= 3 {
        format!(
            "{}:{}:{}",
            parts[0],
            parts[1],
            parts.get(3).copied().unwrap_or("")
        )
    } else {
        name.to_string()
    }
}

/// Mojang-style `inheritsFrom` merge: loader libraries win over vanilla ones with the same coordinates.
pub fn merge(parent: &Value, child: &Value) -> Value {
    let mut out = parent.clone();
    for k in ["mainClass", "id", "minecraftArguments"] {
        if let Some(v) = child.get(k) {
            out[k] = v.clone();
        }
    }

    let child_libs: Vec<Value> = child["libraries"].as_array().cloned().unwrap_or_default();
    let child_keys: HashSet<String> = child_libs.iter().map(lib_key).collect();
    let mut libs = child_libs;
    for lib in parent["libraries"].as_array().into_iter().flatten() {
        if !child_keys.contains(&lib_key(lib)) {
            libs.push(lib.clone());
        }
    }
    out["libraries"] = Value::Array(libs);

    if child.get("arguments").is_some() {
        let mut args = json!({});
        for part in ["jvm", "game"] {
            let mut v: Vec<Value> = parent["arguments"][part]
                .as_array()
                .cloned()
                .unwrap_or_default();
            v.extend(
                child["arguments"][part]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            );
            args[part] = Value::Array(v);
        }
        out["arguments"] = args;
    }
    for k in ["javaVersion", "assetIndex", "assets"] {
        if let Some(v) = child.get(k) {
            out[k] = v.clone();
        }
    }
    out
}
