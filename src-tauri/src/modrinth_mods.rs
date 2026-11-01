//! Search and install Minecraft mods from Modrinth for a specific instance.

use crate::{launcher, AppState};
use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha512};
use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::State;
use url::Url;

const API: &str = "https://api.modrinth.com/v2";

fn str_at(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModResult {
    id: String,
    title: String,
    description: String,
    author: String,
    icon_url: String,
    downloads: u64,
    page_url: String,
}

#[tauri::command]
pub async fn search_modrinth_mods(
    state: State<'_, AppState>,
    id: i64,
    query: String,
) -> Result<Vec<ModResult>, String> {
    let instance = state.db.instance(id).map_err(|e| e.to_string())?;
    if instance.loader == "vanilla" {
        return Err("Choose a mod loader for this instance before searching for mods".into());
    }
    let facets = json!([
        ["project_type:mod"],
        [format!("versions:{}", instance.mc_version)],
        [format!("categories:{}", instance.loader)],
    ])
    .to_string();
    let sort = if query.trim().is_empty() {
        "downloads"
    } else {
        "relevance"
    };
    let response = state
        .http
        .get(format!("{API}/search"))
        .query(&[
            ("query", query.trim()),
            ("facets", facets.as_str()),
            ("limit", "20"),
            ("index", sort),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json::<Value>()
        .await
        .map_err(|e| e.to_string())?;
    Ok(response["hits"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|hit| {
            let slug = str_at(hit, "slug");
            ModResult {
                id: str_at(hit, "project_id"),
                title: str_at(hit, "title"),
                description: str_at(hit, "description"),
                author: str_at(hit, "author"),
                icon_url: str_at(hit, "icon_url"),
                downloads: hit["downloads"].as_u64().unwrap_or_default(),
                page_url: format!("https://modrinth.com/mod/{slug}"),
            }
        })
        .collect())
}

#[tauri::command]
pub async fn search_modrinth_resourcepacks(
    state: State<'_, AppState>,
    id: i64,
    query: String,
) -> Result<Vec<ModResult>, String> {
    let instance = state.db.instance(id).map_err(|e| e.to_string())?;
    let facets = json!([
        ["project_type:resourcepack"],
        [format!("versions:{}", instance.mc_version)],
    ])
    .to_string();
    let sort = if query.trim().is_empty() {
        "downloads"
    } else {
        "relevance"
    };
    let response = state
        .http
        .get(format!("{API}/search"))
        .query(&[
            ("query", query.trim()),
            ("facets", facets.as_str()),
            ("limit", "20"),
            ("index", sort),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json::<Value>()
        .await
        .map_err(|e| e.to_string())?;
    Ok(response["hits"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|hit| {
            let slug = str_at(hit, "slug");
            ModResult {
                id: str_at(hit, "project_id"),
                title: str_at(hit, "title"),
                description: str_at(hit, "description"),
                author: str_at(hit, "author"),
                icon_url: str_at(hit, "icon_url"),
                downloads: hit["downloads"].as_u64().unwrap_or_default(),
                page_url: format!("https://modrinth.com/resourcepack/{slug}"),
            }
        })
        .collect())
}

async fn compatible_version(
    client: &reqwest::Client,
    project_id: &str,
    version_id: Option<&str>,
    mc_version: &str,
    loader: &str,
) -> Result<Value> {
    if let Some(version_id) = version_id.filter(|id| !id.is_empty()) {
        let version = client
            .get(format!("{API}/version/{version_id}"))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let supports_mc = version["game_versions"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| v.as_str() == Some(mc_version));
        let supports_loader = version["loaders"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| v.as_str() == Some(loader));
        if !supports_mc || !supports_loader {
            bail!("A required dependency has no version compatible with Minecraft {mc_version} and {loader}");
        }
        return Ok(version);
    }

    let versions = client
        .get(format!("{API}/project/{project_id}/version"))
        .query(&[
            ("game_versions", json!([mc_version]).to_string()),
            ("loaders", json!([loader]).to_string()),
            ("include_changelog", "false".into()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Value>>()
        .await?;
    versions
        .iter()
        .find(|v| v["version_type"].as_str() == Some("release"))
        .or_else(|| versions.first())
        .cloned()
        .ok_or_else(|| {
            anyhow!("No version of this mod supports Minecraft {mc_version} and {loader}")
        })
}

async fn collect_install_plan(
    client: &reqwest::Client,
    root_project: &str,
    mc_version: &str,
    loader: &str,
) -> Result<Vec<Value>> {
    let mut pending = vec![(root_project.to_string(), None::<String>)];
    let mut seen_projects = HashSet::new();
    let mut seen_versions = HashSet::new();
    let mut plan = Vec::new();
    while let Some((project_id, version_id)) = pending.pop() {
        if !seen_projects.insert(project_id.clone()) {
            continue;
        }
        let version = compatible_version(
            client,
            &project_id,
            version_id.as_deref(),
            mc_version,
            loader,
        )
        .await?;
        let version_key = str_at(&version, "id");
        if version_key.is_empty() {
            bail!("Modrinth returned a version without an ID");
        }
        for dep in version["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|dep| dep["dependency_type"].as_str() == Some("required"))
        {
            let dep_project = str_at(dep, "project_id");
            if !dep_project.is_empty() && !seen_projects.contains(&dep_project) {
                pending.push((dep_project, dep["version_id"].as_str().map(str::to_string)));
            }
        }
        if seen_versions.insert(version_key) {
            plan.push(version);
        }
    }
    Ok(plan)
}

struct DownloadFile {
    name: String,
    url: String,
    sha512: String,
}

fn primary_file(version: &Value, extension: &str) -> Result<DownloadFile> {
    let files = version["files"]
        .as_array()
        .ok_or_else(|| anyhow!("Mod version contains no downloadable files"))?;
    let file = files
        .iter()
        .find(|f| {
            f["primary"].as_bool().unwrap_or(false)
                && f["filename"]
                    .as_str()
                    .unwrap_or_default()
                    .to_lowercase()
                    .ends_with(extension)
        })
        .or_else(|| {
            files.iter().find(|f| {
                f["filename"]
                    .as_str()
                    .unwrap_or_default()
                    .to_lowercase()
                    .ends_with(extension)
            })
        })
        .ok_or_else(|| anyhow!("Modrinth version contains no {extension} file"))?;
    let name = str_at(file, "filename");
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        bail!("Modrinth returned an invalid file name");
    }
    let url = str_at(file, "url");
    let parsed = Url::parse(&url)?;
    if parsed.scheme() != "https" || parsed.host_str() != Some("cdn.modrinth.com") {
        bail!("Modrinth returned an unexpected download host");
    }
    let sha512 = str_at(&file["hashes"], "sha512");
    if sha512.is_empty() {
        bail!("Modrinth did not provide a SHA-512 checksum for {name}");
    }
    Ok(DownloadFile { name, url, sha512 })
}

fn primary_jar(version: &Value) -> Result<DownloadFile> {
    primary_file(version, ".jar")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePackFile {
    name: String,
    size: u64,
}

#[tauri::command]
pub fn list_resource_packs(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Vec<ResourcePackFile>, String> {
    let instance = state.db.instance(id).map_err(|e| e.to_string())?;
    let dir = launcher::game_dir(&state.root, &instance).join("resourcepacks");
    let mut packs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.to_lowercase().ends_with(".zip") {
                packs.push(ResourcePackFile {
                    size: entry
                        .metadata()
                        .map(|metadata| metadata.len())
                        .unwrap_or_default(),
                    name,
                });
            }
        }
    }
    packs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(packs)
}

#[tauri::command]
pub fn delete_resource_pack(
    state: State<'_, AppState>,
    id: i64,
    name: String,
) -> Result<(), String> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || !name.to_lowercase().ends_with(".zip")
    {
        return Err("Invalid resource pack file name".into());
    }
    let instance = state.db.instance(id).map_err(|e| e.to_string())?;
    let path = launcher::game_dir(&state.root, &instance)
        .join("resourcepacks")
        .join(name);
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_modrinth_resourcepack(
    state: State<'_, AppState>,
    id: i64,
    project_id: String,
) -> Result<String, String> {
    let result: Result<String> = async {
        let instance = state.db.instance(id)?;
        let version = compatible_version(&state.http, &project_id, None, &instance.mc_version, "minecraft").await?;
        let file = primary_file(&version, ".zip")?;
        let dir = launcher::game_dir(&state.root, &instance).join("resourcepacks");
        tokio::fs::create_dir_all(&dir).await?;
        let destination = dir.join(&file.name);
        if tokio::fs::try_exists(&destination).await? {
            let bytes = tokio::fs::read(&destination).await?;
            if format!("{:x}", Sha512::digest(&bytes)) == file.sha512 {
                return Ok(file.name);
            }
            bail!("{} already exists with different contents. Remove it before installing this version.", file.name);
        }

        let bytes = state.http.get(&file.url).send().await?.error_for_status()?.bytes().await?;
        if format!("{:x}", Sha512::digest(&bytes)) != file.sha512 {
            bail!("Checksum verification failed for {}", file.name);
        }
        let staging = dir.join(format!(".sunrise-resourcepack-{}-{}", id, SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()));
        tokio::fs::write(&staging, &bytes).await?;
        if let Err(error) = tokio::fs::rename(&staging, &destination).await {
            let _ = tokio::fs::remove_file(&staging).await;
            return Err(error.into());
        }
        Ok(file.name)
    }
    .await;
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_modrinth_mod(
    state: State<'_, AppState>,
    id: i64,
    project_id: String,
) -> Result<Vec<String>, String> {
    let result: Result<Vec<String>> = async {
        let instance = state.db.instance(id)?;
        if instance.loader == "vanilla" { bail!("Choose a mod loader for this instance before installing mods"); }
        let plan = collect_install_plan(&state.http, &project_id, &instance.mc_version, &instance.loader).await?;

        let files = plan.iter().map(primary_jar).collect::<Result<Vec<_>>>()?;
        let mut names = HashSet::new();
        for file in &files {
            if !names.insert(file.name.to_lowercase()) { bail!("Two selected mods use the same file name: {}", file.name); }
        }

        let instance_dir = launcher::game_dir(&state.root, &instance);
        let mods_dir = instance_dir.join("mods");
        tokio::fs::create_dir_all(&mods_dir).await?;
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let staging = mods_dir.join(format!(".sunrise-modrinth-{id}-{nonce}"));
        tokio::fs::create_dir(&staging).await?;
        let stage_result: Result<Vec<String>> = async {
            for file in &files {
                let existing = mods_dir.join(&file.name);
                if tokio::fs::try_exists(&existing).await? {
                    let bytes = tokio::fs::read(&existing).await?;
                    if format!("{:x}", Sha512::digest(&bytes)) == file.sha512 { continue; }
                    bail!("{} already exists in this instance with different contents. Remove it before installing this version.", file.name);
                }
                let bytes = state.http.get(&file.url).send().await?.error_for_status()?.bytes().await?;
                let digest = format!("{:x}", Sha512::digest(&bytes));
                if digest != file.sha512 { bail!("Checksum verification failed for {}", file.name); }
                tokio::fs::write(staging.join(&file.name), &bytes).await?;
            }
            let mut installed = Vec::new();
            for file in &files {
                let staged = staging.join(&file.name);
                if tokio::fs::try_exists(&staged).await? {
                    tokio::fs::rename(staged, mods_dir.join(&file.name)).await
                        .with_context(|| format!("Could not install {}", file.name))?;
                    installed.push(file.name.clone());
                }
            }
            Ok(installed)
        }.await;
        let _ = tokio::fs::remove_dir_all(&staging).await;
        stage_result
    }.await;
    result.map_err(|e| e.to_string())
}
