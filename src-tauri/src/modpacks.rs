//! Modpacks: browse + install from Modrinth (.mrpack) and CurseForge (manifest.json zips).
//!
//! An install creates a normal instance, downloads the pack's files into its folder, extracts the
//! `overrides`, and sets the Minecraft version + mod loader. Pressing Play then installs the game itself.

use crate::{
    commands::{create_instance_inner, emit_state},
    db::{Instance, NewInstance, Settings},
    launcher::{self, Reporter},
    AppState,
};
use anyhow::{anyhow, bail, Result};
use futures_util::{stream, StreamExt, TryStreamExt};
use reqwest::{Client, Method, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha512};
use std::{
    collections::{HashMap, HashSet},
    fs, io,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tracing::{info, warn};
use url::Url;

const MR_API: &str = "https://api.modrinth.com/v2";
const CF_API: &str = "https://api.curseforge.com/v1";
const CF_MINECRAFT: u32 = 432;
const CF_MODPACKS: u32 = 4471;
const NO_KEY: &str =
    "Add your CurseForge API key in Settings first (free at console.curseforge.com). Modrinth works without a key.";

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}
fn st(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").to_string()
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn cf_key(s: &Settings) -> String {
    std::env::var("SUNRISE_CF_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| s.curseforge_key.clone())
        .trim()
        .to_string()
}
fn norm_loader(s: &str) -> String {
    match s.to_lowercase().as_str() {
        "fabric" => "fabric",
        "forge" => "forge",
        "neoforge" => "neoforge",
        _ => "vanilla",
    }
    .into()
}

// ───────────────────────── types sent to the UI ─────────────────────────

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PackResult {
    pub source: String,
    pub id: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub icon_url: String,
    pub downloads: u64,
    pub page_url: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PackVersion {
    pub id: String,
    pub name: String,
    pub mc_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub date: String,
}

/// A CurseForge file whose author disabled third-party downloads: the user has to fetch it manually.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Blocked {
    pub name: String,
    pub file_name: String,
    pub url: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BlockedEvent {
    id: i64,
    files: Vec<Blocked>,
}

// ───────────────────────── Modrinth API ─────────────────────────

async fn modrinth_search(client: &Client, query: &str, offset: u32) -> Result<Vec<PackResult>> {
    let index = if query.trim().is_empty() {
        "downloads"
    } else {
        "relevance"
    };
    let v: Value = client
        .get(format!("{MR_API}/search"))
        .query(&[
            ("query", query.to_string()),
            ("facets", r#"[["project_type:modpack"]]"#.to_string()),
            ("limit", "20".to_string()),
            ("offset", offset.to_string()),
            ("index", index.to_string()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(v["hits"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|h| PackResult {
            source: "modrinth".into(),
            id: st(h, "project_id"),
            title: st(h, "title"),
            description: st(h, "description"),
            author: st(h, "author"),
            icon_url: st(h, "icon_url"),
            downloads: h["downloads"].as_u64().unwrap_or(0),
            page_url: format!("https://modrinth.com/modpack/{}", st(h, "slug")),
        })
        .collect())
}

async fn modrinth_versions(client: &Client, project_id: &str) -> Result<Vec<PackVersion>> {
    let v: Value = client
        .get(format!("{MR_API}/project/{project_id}/version"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let strs = |x: &Value| -> Vec<String> {
        x.as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect()
    };
    Ok(v.as_array()
        .into_iter()
        .flatten()
        .map(|e| {
            let name = st(e, "name");
            let num = st(e, "version_number");
            PackVersion {
                id: st(e, "id"),
                name: if name.is_empty() {
                    num
                } else if num.is_empty() || name.contains(&num) {
                    name
                } else {
                    format!("{name} ({num})")
                },
                mc_versions: strs(&e["game_versions"]),
                loaders: strs(&e["loaders"]),
                date: st(e, "date_published").chars().take(10).collect(),
            }
        })
        .collect())
}

struct FileInfo {
    url: String,
    file_name: String,
    mc: String,
    loader: String,
}

async fn modrinth_file(client: &Client, version_id: &str) -> Result<FileInfo> {
    let v: Value = client
        .get(format!("{MR_API}/version/{version_id}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let files = v["files"]
        .as_array()
        .ok_or_else(|| anyhow!("this version has no files"))?;
    let f = files
        .iter()
        .find(|f| f["primary"].as_bool().unwrap_or(false))
        .or_else(|| files.first())
        .ok_or_else(|| anyhow!("this version has no files"))?;
    Ok(FileInfo {
        url: st(f, "url"),
        file_name: st(f, "filename"),
        mc: v["game_versions"][0].as_str().unwrap_or("").to_string(),
        loader: norm_loader(v["loaders"][0].as_str().unwrap_or("")),
    })
}

// ───────────────────────── CurseForge API (needs the user's own key) ─────────────────────────

fn cf_req(client: &Client, key: &str, method: Method, path: &str) -> RequestBuilder {
    client
        .request(method, format!("{CF_API}{path}"))
        .header("x-api-key", key)
        .header("Accept", "application/json")
}

async fn cf_json(rb: RequestBuilder) -> Result<Value> {
    let resp = rb.send().await?;
    let code = resp.status().as_u16();
    if code == 401 || code == 403 {
        bail!("CurseForge rejected the API key – check it in Settings");
    }
    Ok(resp.error_for_status()?.json().await?)
}

async fn modpack_banner_url(
    client: &Client,
    source: &str,
    project_id: &str,
    key: &str,
) -> Result<String> {
    let project = match source {
        "modrinth" => {
            client
                .get(format!("{MR_API}/project/{project_id}"))
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?
        }
        "curseforge" => cf_json(cf_req(
            client,
            key,
            Method::GET,
            &format!("/mods/{project_id}"),
        ))
        .await?["data"]
            .clone(),
        _ => return Ok(String::new()),
    };
    let gallery = project["gallery"]
        .as_array()
        .or_else(|| project["screenshots"].as_array());
    let Some(images) = gallery else {
        return Ok(String::new());
    };
    let featured = images
        .iter()
        .find(|image| image["featured"].as_bool().unwrap_or(false));
    let image = featured.or_else(|| images.first());
    let url = image
        .and_then(|image| {
            image["url"]
                .as_str()
                .or_else(|| image["thumbnailUrl"].as_str())
        })
        .unwrap_or_default();
    if url.starts_with("https://") {
        Ok(url.to_string())
    } else {
        Ok(String::new())
    }
}

async fn cf_search(
    client: &Client,
    key: &str,
    query: &str,
    offset: u32,
) -> Result<Vec<PackResult>> {
    let v = cf_json(cf_req(client, key, Method::GET, "/mods/search").query(&[
        ("gameId", CF_MINECRAFT.to_string()),
        ("classId", CF_MODPACKS.to_string()),
        ("searchFilter", query.to_string()),
        ("sortField", "2".to_string()), // popularity
        ("sortOrder", "desc".to_string()),
        ("pageSize", "20".to_string()),
        ("index", offset.to_string()),
    ]))
    .await?;
    Ok(v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|h| PackResult {
            source: "curseforge".into(),
            id: h["id"].as_u64().unwrap_or(0).to_string(),
            title: st(h, "name"),
            description: st(h, "summary"),
            author: h["authors"][0]["name"].as_str().unwrap_or("").to_string(),
            icon_url: h["logo"]["thumbnailUrl"].as_str().unwrap_or("").to_string(),
            downloads: h["downloadCount"]
                .as_u64()
                .or_else(|| h["downloadCount"].as_f64().map(|f| f as u64))
                .unwrap_or(0),
            page_url: h["links"]["websiteUrl"].as_str().unwrap_or("").to_string(),
        })
        .collect())
}

async fn cf_versions(client: &Client, key: &str, mod_id: &str) -> Result<Vec<PackVersion>> {
    let v = cf_json(
        cf_req(client, key, Method::GET, &format!("/mods/{mod_id}/files"))
            .query(&[("pageSize", "50")]),
    )
    .await?;
    let mut out: Vec<PackVersion> = v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            let gv: Vec<String> = f["gameVersions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect();
            let is_ver = |s: &String| {
                s.chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
            };
            PackVersion {
                id: f["id"].as_u64().unwrap_or(0).to_string(),
                name: {
                    let n = st(f, "displayName");
                    if n.is_empty() {
                        st(f, "fileName")
                    } else {
                        n
                    }
                },
                mc_versions: gv.iter().filter(|s| is_ver(s)).cloned().collect(),
                loaders: gv
                    .iter()
                    .filter(|s| {
                        !is_ver(s) && !s.starts_with("Java") && *s != "Client" && *s != "Server"
                    })
                    .cloned()
                    .collect(),
                date: st(f, "fileDate").chars().take(10).collect(),
            }
        })
        .collect();
    out.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(out)
}

async fn cf_file_info(client: &Client, key: &str, mod_id: &str, file_id: &str) -> Result<FileInfo> {
    let v = cf_json(cf_req(
        client,
        key,
        Method::GET,
        &format!("/mods/{mod_id}/files/{file_id}"),
    ))
    .await?;
    let f = &v["data"];
    let mut url = f["downloadUrl"].as_str().unwrap_or("").to_string();
    if url.is_empty() {
        let u = cf_json(cf_req(
            client,
            key,
            Method::GET,
            &format!("/mods/{mod_id}/files/{file_id}/download-url"),
        ))
        .await;
        url = u
            .ok()
            .and_then(|x| x["data"].as_str().map(String::from))
            .unwrap_or_default();
    }
    if url.is_empty() {
        bail!("The author disabled third-party downloads for this pack. Download the .zip from its CurseForge page and use “Import file”.");
    }
    let gv: Vec<String> = f["gameVersions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(String::from)
        .collect();
    Ok(FileInfo {
        url,
        file_name: st(f, "fileName"),
        mc: gv
            .iter()
            .find(|s| {
                s.chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
            })
            .cloned()
            .unwrap_or_default(),
        loader: gv
            .iter()
            .map(|s| norm_loader(s))
            .find(|l| l != "vanilla")
            .unwrap_or_else(|| "vanilla".into()),
    })
}

// ───────────────────────── reading a pack file ─────────────────────────

enum PackKind {
    Mr(Value),
    Cf(Value),
}

#[derive(Clone, Debug)]
struct PackMeta {
    name: String,
    mc: String,
    loader: String,
    loader_version: String,
}

fn read_pack(path: &Path) -> Result<PackKind> {
    let mut z = zip::ZipArchive::new(fs::File::open(path)?)?;
    let mut text = String::new();
    if let Ok(mut f) = z.by_name("modrinth.index.json") {
        f.read_to_string(&mut text)?;
        return Ok(PackKind::Mr(serde_json::from_str(&text)?));
    }
    if let Ok(mut f) = z.by_name("manifest.json") {
        f.read_to_string(&mut text)?;
        return Ok(PackKind::Cf(serde_json::from_str(&text)?));
    }
    bail!("This isn't a modpack (expected modrinth.index.json in a .mrpack, or manifest.json in a CurseForge zip)")
}

fn pack_info(k: &PackKind) -> Result<PackMeta> {
    match k {
        PackKind::Mr(v) => {
            let deps = &v["dependencies"];
            let mc = deps["minecraft"]
                .as_str()
                .ok_or_else(|| anyhow!("modpack has no Minecraft version"))?
                .to_string();
            let (loader, lv) = if let Some(x) = deps["fabric-loader"].as_str() {
                ("fabric", x.to_string())
            } else if let Some(x) = deps["neoforge"].as_str() {
                ("neoforge", x.to_string())
            } else if let Some(x) = deps["forge"].as_str() {
                ("forge", x.to_string())
            } else if deps.get("quilt-loader").is_some() {
                bail!("Quilt modpacks aren't supported yet")
            } else {
                ("vanilla", String::new())
            };
            Ok(PackMeta {
                name: st(v, "name"),
                mc,
                loader: loader.into(),
                loader_version: lv,
            })
        }
        PackKind::Cf(v) => {
            let mc = v["minecraft"]["version"]
                .as_str()
                .ok_or_else(|| anyhow!("modpack has no Minecraft version"))?
                .to_string();
            let loaders = v["minecraft"]["modLoaders"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let pick = loaders
                .iter()
                .find(|l| l["primary"].as_bool().unwrap_or(false))
                .or_else(|| loaders.first());
            let (loader, lv) = match pick.and_then(|l| l["id"].as_str()) {
                None => ("vanilla".to_string(), String::new()),
                Some(id) => {
                    let (name, ver) = id.split_once('-').unwrap_or((id, ""));
                    match name {
                        "forge" | "fabric" | "neoforge" => (name.to_string(), ver.to_string()),
                        "quilt" => bail!("Quilt modpacks aren't supported yet"),
                        other => bail!("Unsupported mod loader “{other}”"),
                    }
                }
            };
            Ok(PackMeta {
                name: st(v, "name"),
                mc,
                loader,
                loader_version: lv,
            })
        }
    }
}

// ───────────────────────── downloading pack files ─────────────────────────

struct Job {
    url: String,
    dest: PathBuf,
    sha512: Option<String>,
}

async fn fetch_job(client: &Client, job: &Job) -> Result<()> {
    if let Some(p) = job.dest.parent() {
        tokio::fs::create_dir_all(p).await?;
    }
    let bytes = client
        .get(&job.url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if let Some(h) = &job.sha512 {
        let got = hex(&Sha512::digest(&bytes));
        if !got.eq_ignore_ascii_case(h) {
            bail!("checksum mismatch for {}", job.dest.display());
        }
    }
    tokio::fs::write(&job.dest, &bytes).await?;
    Ok(())
}

async fn run_jobs(
    client: &Client,
    rep: &Reporter,
    jobs: Vec<Job>,
    label: &str,
    from: f32,
    to: f32,
) -> Result<()> {
    let total = jobs.len().max(1);
    let done = Arc::new(AtomicUsize::new(0));
    stream::iter(jobs.into_iter().map(|job| {
        let client = client.clone();
        let done = done.clone();
        let rep = rep.clone();
        let label = label.to_string();
        async move {
            let mut res = fetch_job(&client, &job).await;
            if res.is_err() {
                res = fetch_job(&client, &job).await; // one retry
            }
            res.map_err(|e| anyhow!("{}: {e}", job.url))?;
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            rep.set(
                format!("{label} ({d}/{total})"),
                from + (to - from) * d as f32 / total as f32,
            );
            Ok::<(), anyhow::Error>(())
        }
    }))
    .buffer_unordered(12)
    .try_collect::<Vec<()>>()
    .await?;
    Ok(())
}

/// Only plain relative paths inside the instance folder.
fn safe_rel(p: &str) -> Option<PathBuf> {
    let p = p.replace('\\', "/");
    if p.is_empty() || p.starts_with('/') || p.contains(':') {
        return None;
    }
    let path = PathBuf::from(&p);
    if path
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        Some(path)
    } else {
        None
    }
}

fn host_allowed(u: &str, hosts: &[&str]) -> bool {
    Url::parse(u)
        .ok()
        .filter(|x| x.scheme() == "https")
        .and_then(|x| {
            x.host_str().map(|h| {
                hosts
                    .iter()
                    .any(|a| h == *a || h.ends_with(&format!(".{a}")))
            })
        })
        .unwrap_or(false)
}

fn extract_overrides(zip_path: &Path, dir: &Path, prefixes: &[String]) -> Result<()> {
    let mut z = zip::ZipArchive::new(fs::File::open(zip_path)?)?;
    for prefix in prefixes {
        for i in 0..z.len() {
            let mut e = z.by_index(i)?;
            let Some(enc) = e.enclosed_name() else {
                continue;
            };
            let name = enc.to_string_lossy().replace('\\', "/");
            let Some(rel) = name.strip_prefix(prefix.as_str()) else {
                continue;
            };
            let rel = rel.trim_start_matches('/');
            if rel.is_empty() {
                continue;
            }
            let dest = dir.join(rel);
            if e.is_dir() {
                fs::create_dir_all(&dest)?;
                continue;
            }
            if let Some(p) = dest.parent() {
                fs::create_dir_all(p)?;
            }
            let mut out = fs::File::create(&dest)?;
            io::copy(&mut e, &mut out)?;
        }
    }
    Ok(())
}

async fn install_mr(
    client: &Client,
    rep: &Reporter,
    zip: &Path,
    dir: &Path,
    index: &Value,
) -> Result<()> {
    const HOSTS: [&str; 4] = [
        "cdn.modrinth.com",
        "github.com",
        "raw.githubusercontent.com",
        "gitlab.com",
    ];
    let mut jobs = Vec::new();
    for f in index["files"].as_array().into_iter().flatten() {
        if f["env"]["client"].as_str() == Some("unsupported") {
            continue; // server-only
        }
        let path = st(f, "path");
        let rel = safe_rel(&path).ok_or_else(|| anyhow!("unsafe path in modpack: {path}"))?;
        let url = f["downloads"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .find(|u| host_allowed(u, &HOSTS))
            .ok_or_else(|| anyhow!("no allowed download source for {path}"))?
            .to_string();
        jobs.push(Job {
            url,
            dest: dir.join(rel),
            sha512: f["hashes"]["sha512"].as_str().map(String::from),
        });
    }
    run_jobs(client, rep, jobs, "Mods", 0.10, 0.92).await?;

    rep.set("Extracting overrides…", 0.94);
    let (z, d) = (zip.to_path_buf(), dir.to_path_buf());
    tokio::task::spawn_blocking(move || {
        extract_overrides(
            &z,
            &d,
            &["overrides/".to_string(), "client-overrides/".to_string()],
        )
    })
    .await??;
    Ok(())
}

async fn install_cf(
    client: &Client,
    rep: &Reporter,
    zip: &Path,
    dir: &Path,
    manifest: &Value,
    key: &str,
) -> Result<Vec<Blocked>> {
    if key.is_empty() {
        bail!("{NO_KEY}");
    }
    let files: Vec<(u64, u64)> = manifest["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| Some((f["projectID"].as_u64()?, f["fileID"].as_u64()?)))
        .collect();

    rep.set("Resolving CurseForge files…", 0.05);
    let file_ids: Vec<u64> = files.iter().map(|f| f.1).collect();
    let mut infos: HashMap<u64, Value> = HashMap::new();
    for chunk in file_ids.chunks(200) {
        let v = cf_json(
            cf_req(client, key, Method::POST, "/mods/files").json(&json!({ "fileIds": chunk })),
        )
        .await?;
        for f in v["data"].as_array().into_iter().flatten() {
            if let Some(id) = f["id"].as_u64() {
                infos.insert(id, f.clone());
            }
        }
    }
    let mod_ids: Vec<u64> = files
        .iter()
        .map(|f| f.0)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let mut mods: HashMap<u64, Value> = HashMap::new();
    for chunk in mod_ids.chunks(200) {
        let v =
            cf_json(cf_req(client, key, Method::POST, "/mods").json(&json!({ "modIds": chunk })))
                .await?;
        for m in v["data"].as_array().into_iter().flatten() {
            if let Some(id) = m["id"].as_u64() {
                mods.insert(id, m.clone());
            }
        }
    }

    let mut jobs = Vec::new();
    let mut blocked = Vec::new();
    let mut seen = HashSet::new();
    for (pid, fid) in files {
        let m = mods.get(&pid);
        let mod_name = m
            .map(|m| st(m, "name"))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("Project {pid}"));
        let site = m
            .and_then(|m| m["links"]["websiteUrl"].as_str())
            .unwrap_or("https://www.curseforge.com")
            .trim_end_matches('/')
            .to_string();
        let info = infos.get(&fid);
        let file_name = info.map(|i| st(i, "fileName")).unwrap_or_default();
        let url = info
            .and_then(|i| i["downloadUrl"].as_str())
            .unwrap_or("")
            .to_string();

        // Authors can forbid third-party downloads; we respect that and ask the user to fetch the file.
        if url.is_empty() || file_name.is_empty() {
            blocked.push(Blocked {
                name: mod_name,
                file_name,
                url: format!("{site}/files/{fid}"),
            });
            continue;
        }
        if !host_allowed(&url, &["forgecdn.net"]) {
            bail!("unexpected download host for {mod_name}");
        }
        let sub = match m.and_then(|m| m["classId"].as_u64()) {
            Some(12) => "resourcepacks",
            Some(6552) => "shaderpacks",
            _ => "mods",
        };
        let Some(rel) = safe_rel(&file_name) else {
            bail!("unsafe file name: {file_name}")
        };
        let dest = dir.join(sub).join(rel);
        if seen.insert(dest.clone()) {
            jobs.push(Job {
                url,
                dest,
                sha512: None,
            });
        }
    }
    run_jobs(client, rep, jobs, "Mods", 0.10, 0.92).await?;

    rep.set("Extracting overrides…", 0.94);
    let over = manifest["overrides"]
        .as_str()
        .unwrap_or("overrides")
        .trim_matches('/')
        .to_string();
    let (z, d) = (zip.to_path_buf(), dir.to_path_buf());
    tokio::task::spawn_blocking(move || extract_overrides(&z, &d, &[format!("{over}/")])).await??;
    Ok(blocked)
}

async fn install_zip(
    client: &Client,
    rep: &Reporter,
    zip: &Path,
    dir: &Path,
    key: &str,
) -> Result<(PackMeta, Vec<Blocked>)> {
    let z = zip.to_path_buf();
    let kind = tokio::task::spawn_blocking(move || read_pack(&z)).await??;
    let meta = pack_info(&kind)?;
    rep.set(format!("Installing {}…", meta.name), 0.08);
    fs::create_dir_all(dir)?;
    let blocked = match &kind {
        PackKind::Mr(index) => {
            install_mr(client, rep, zip, dir, index).await?;
            Vec::new()
        }
        PackKind::Cf(manifest) => install_cf(client, rep, zip, dir, manifest, key).await?,
    };
    Ok((meta, blocked))
}

// ───────────────────────── the install task ─────────────────────────

enum Src {
    Remote { url: String, file_name: String },
    Local(PathBuf),
}

async fn run_pack_install(app: AppHandle, inst: Instance, src: Src, key: String) {
    let state = app.state::<AppState>();
    let rep = Reporter::new(app.clone(), inst.id);
    emit_state(&app, inst.id, "installing", "", None);
    let dir = launcher::game_dir(&state.root, &inst);

    let res: Result<(PackMeta, Vec<Blocked>)> = async {
        let zip = match src {
            Src::Local(p) => p,
            Src::Remote { url, file_name } => {
                if !host_allowed(&url, &["modrinth.com", "forgecdn.net", "curseforge.com"]) {
                    bail!("unexpected download host");
                }
                let cache = state.root.join("cache").join("modpacks");
                tokio::fs::create_dir_all(&cache).await?;
                let safe: String = file_name
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                let zip = cache.join(safe);
                rep.set("Downloading modpack…", 0.02);
                // always fetch fresh: the cache name only contains the version id
                let _ = tokio::fs::remove_file(&zip).await;
                launcher::download(&state.http, &url, &zip).await?;
                zip
            }
        };
        let (meta, blocked) = install_zip(&state.http, &rep, &zip, &dir, &key).await?;
        let mut i = state.db.instance(inst.id)?;
        i.mc_version = meta.mc.clone();
        i.loader = meta.loader.clone();
        i.loader_version = meta.loader_version.clone();
        state.db.update_instance(&i)?;
        Ok((meta, blocked))
    }
    .await;

    match res {
        Ok((meta, blocked)) => {
            info!(instance = inst.id, pack = %meta.name, blocked = blocked.len(), "modpack installed");
            if !blocked.is_empty() {
                let _ = app.emit(
                    "modpack-blocked",
                    BlockedEvent {
                        id: inst.id,
                        files: blocked,
                    },
                );
            }
            rep.set("Modpack installed", 1.0);
            emit_state(
                &app,
                inst.id,
                "idle",
                "Modpack installed – press Play to download the game",
                None,
            );
        }
        Err(e) => {
            warn!(instance = inst.id, error = %e, "modpack install failed");
            let _ = state.db.delete_instance(inst.id);
            if dir.parent().is_some() {
                let _ = tokio::fs::remove_dir_all(&dir).await;
            }
            emit_state(
                &app,
                inst.id,
                "error",
                &format!("Modpack install failed: {e}"),
                None,
            );
        }
    }
}

// ───────────────────────── commands ─────────────────────────

#[tauri::command]
pub async fn search_modpacks(
    state: State<'_, AppState>,
    source: String,
    query: String,
    offset: u32,
) -> Result<Vec<PackResult>, String> {
    match source.as_str() {
        "modrinth" => modrinth_search(&state.http, &query, offset)
            .await
            .map_err(err),
        "curseforge" => {
            let key = cf_key(&state.db.settings());
            if key.is_empty() {
                return Err(NO_KEY.into());
            }
            cf_search(&state.http, &key, &query, offset)
                .await
                .map_err(err)
        }
        _ => Err("Unknown source".into()),
    }
}

#[tauri::command]
pub async fn modpack_versions(
    state: State<'_, AppState>,
    source: String,
    project_id: String,
) -> Result<Vec<PackVersion>, String> {
    match source.as_str() {
        "modrinth" => modrinth_versions(&state.http, &project_id)
            .await
            .map_err(err),
        "curseforge" => {
            let key = cf_key(&state.db.settings());
            if key.is_empty() {
                return Err(NO_KEY.into());
            }
            cf_versions(&state.http, &key, &project_id)
                .await
                .map_err(err)
        }
        _ => Err("Unknown source".into()),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPackReq {
    pub source: String,
    pub project_id: String,
    pub version_id: String,
    pub name: String,
}

/// Creates the instance right away and installs the pack in the background (progress events as usual).
#[tauri::command]
pub async fn install_modpack(
    app: AppHandle,
    state: State<'_, AppState>,
    req: InstallPackReq,
) -> Result<Instance, String> {
    let key = cf_key(&state.db.settings());
    // A missing gallery or a temporary gallery API failure must not prevent installation.
    let banner_url = modpack_banner_url(&state.http, &req.source, &req.project_id, &key)
        .await
        .unwrap_or_default();
    let fi = match req.source.as_str() {
        "modrinth" => modrinth_file(&state.http, &req.version_id)
            .await
            .map_err(err)?,
        "curseforge" => {
            if key.is_empty() {
                return Err(NO_KEY.into());
            }
            cf_file_info(&state.http, &key, &req.project_id, &req.version_id)
                .await
                .map_err(err)?
        }
        _ => return Err("Unknown source".into()),
    };
    let name = if req.name.trim().is_empty() {
        "Modpack".to_string()
    } else {
        req.name.trim().to_string()
    };
    let inst = create_instance_inner(
        &state,
        &NewInstance {
            name,
            mc_version: if fi.mc.is_empty() {
                "unknown".into()
            } else {
                fi.mc
            },
            loader: fi.loader,
            loader_version: String::new(),
            java_path: String::new(),
            ram_mb: 0,
            color: String::new(),
            banner_url,
            icon_url: String::new(),
            dir_parent: String::new(),
        },
    )?;
    let src = Src::Remote {
        url: fi.url,
        file_name: format!("{}-{}", req.version_id, fi.file_name),
    };
    let (app2, inst2) = (app.clone(), inst.clone());
    tauri::async_runtime::spawn(async move {
        run_pack_install(app2, inst2, src, key).await;
    });
    Ok(inst)
}

#[tauri::command]
pub async fn pick_modpack_file(app: AppHandle) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Choose a modpack")
            .add_filter("Modpack", &["mrpack", "zip"])
            .blocking_pick_file()
    })
    .await
    .ok()
    .flatten()
    .and_then(|p| p.into_path().ok())
    .map(|p| p.to_string_lossy().to_string())
}

/// Install a modpack from a local .mrpack / CurseForge .zip.
#[tauri::command]
pub async fn import_modpack_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    name: String,
) -> Result<Instance, String> {
    let p = PathBuf::from(&path);
    let p2 = p.clone();
    let kind = tokio::task::spawn_blocking(move || read_pack(&p2))
        .await
        .map_err(err)?
        .map_err(err)?;
    let meta = pack_info(&kind).map_err(err)?;
    let inst = create_instance_inner(
        &state,
        &NewInstance {
            name: if name.trim().is_empty() {
                meta.name.clone()
            } else {
                name.trim().to_string()
            },
            mc_version: meta.mc,
            loader: meta.loader,
            loader_version: meta.loader_version,
            java_path: String::new(),
            ram_mb: 0,
            color: String::new(),
            banner_url: String::new(),
            icon_url: String::new(),
            dir_parent: String::new(),
        },
    )?;
    let key = cf_key(&state.db.settings());
    let (app2, inst2) = (app.clone(), inst.clone());
    tauri::async_runtime::spawn(async move {
        run_pack_install(app2, inst2, Src::Local(p), key).await;
    });
    Ok(inst)
}
