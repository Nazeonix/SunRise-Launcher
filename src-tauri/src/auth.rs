//! Microsoft / Xbox / Minecraft authentication – OAuth 2.0 authorization-code flow with PKCE
//! and a loopback redirect (http://localhost:<port>).
//!
//! Chain: Microsoft token -> Xbox Live (XBL) -> XSTS -> Minecraft services -> profile.

use crate::db::Account;
use anyhow::{anyhow, bail, Result};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use rand::RngCore;
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tracing::info;
use url::Url;

const SCOPE: &str = "XboxLive.signin offline_access";
const AUTH_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const MC_SESSION_ROOT: &str = "https://sessionserver.mojang.com/";

/// What the UI shows.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub id: i64,
    pub username: String,
    pub uuid: String,
    pub provider: String,
    pub skin_url: Option<String>,
}

impl From<&Account> for AccountInfo {
    fn from(a: &Account) -> Self {
        Self {
            id: a.id,
            username: a.username.clone(),
            uuid: a.uuid.clone(),
            provider: a.provider.clone(),
            skin_url: (!a.skin_url.is_empty()).then(|| a.skin_url.clone()),
        }
    }
}

/// What the game process needs.
#[derive(Clone)]
pub struct AuthInfo {
    pub name: String,
    pub uuid: String,
    pub token: String,
    pub user_type: String,
    pub api_root: Option<String>,
    pub skin_url: Option<String>,
}

impl AuthInfo {
    pub fn offline(name: &str) -> Self {
        Self {
            name: name.to_string(),
            uuid: offline_uuid(name),
            token: "0".into(),
            user_type: "legacy".into(),
            api_root: None,
            skin_url: None,
        }
    }
}

/// Same algorithm Java uses for offline UUIDs.
fn offline_uuid(name: &str) -> String {
    let mut b = md5::compute(format!("OfflinePlayer:{name}")).0;
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

// ───────────────────────── helpers ─────────────────────────

fn s(v: &Value, k: &str) -> Result<String> {
    v[k].as_str()
        .map(String::from)
        .ok_or_else(|| anyhow!("unexpected response: missing `{k}`"))
}

fn oauth_error(v: &Value) -> String {
    let code = v["error"].as_str().unwrap_or("error");
    let desc = v["error_description"].as_str().unwrap_or("");
    let hint = if desc.contains("AADSTS700016") || desc.contains("AADSTS900144") {
        " – the client ID was not found. Check it in Settings."
    } else if desc.contains("AADSTS50011") {
        " – add http://localhost as a redirect URI (platform: Mobile and desktop applications)."
    } else if desc.contains("AADSTS70011") {
        " – the app must support personal Microsoft accounts."
    } else if desc.contains("AADSTS70002") || desc.contains("AADSTS7000218") {
        " – enable \"Allow public client flows\" for your Azure app."
    } else {
        ""
    };
    format!("{code}: {}{hint}", desc.lines().next().unwrap_or(""))
}

fn xsts_error(code: Option<u64>) -> String {
    match code {
        Some(2148916233) => {
            "This Microsoft account has no Xbox profile. Sign in once at xbox.com to create it."
                .into()
        }
        Some(2148916235) => "Xbox Live is not available in your country/region.".into(),
        Some(2148916236) | Some(2148916237) => {
            "This account needs adult verification on the Xbox site.".into()
        }
        Some(2148916238) => {
            "This is a child account – add it to a Microsoft Family group first.".into()
        }
        _ => "Xbox Live authorisation was refused.".into(),
    }
}

fn random_b64(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}

async fn respond(sock: &mut TcpStream, status: &str, title: &str, msg: &str) {
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Sunrise</title></head>\
         <body style=\"font-family:system-ui,sans-serif;background:#EEE9FB;color:#1D1A3A;display:grid;place-items:center;height:100vh;margin:0\">\
         <div style=\"background:#F8F5FF;border:1px solid #CBC3E3;border-radius:16px;padding:32px 40px;text-align:center\">\
         <h2 style=\"margin:0 0 8px\">{title}</h2><p style=\"margin:0;color:#4A4565\">{msg}</p></div></body></html>"
    );
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = sock.write_all(resp.as_bytes()).await;
    let _ = sock.shutdown().await;
}

/// Waits for the browser to hit http://localhost:<port>/?code=…&state=…
async fn wait_for_code(listener: TcpListener, state: &str, cancel: &AtomicBool) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if cancel.load(Ordering::Relaxed) {
            bail!("Sign-in cancelled");
        }
        if Instant::now() > deadline {
            bail!("Sign-in timed out");
        }
        let accepted = tokio::time::timeout(Duration::from_millis(500), listener.accept()).await;
        let (mut sock, _) = match accepted {
            Ok(Ok(x)) => x,
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => continue,
        };
        let mut buf = vec![0u8; 8192];
        let n = sock.read(&mut buf).await.unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let path = req
            .lines()
            .next()
            .unwrap_or("")
            .split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .to_string();
        let Ok(url) = Url::parse(&format!("http://localhost{path}")) else {
            respond(&mut sock, "400 Bad Request", "Bad request", "").await;
            continue;
        };
        let q: HashMap<String, String> = url.query_pairs().into_owned().collect();

        if let Some(err) = q.get("error") {
            let desc = q.get("error_description").cloned().unwrap_or_default();
            respond(
                &mut sock,
                "200 OK",
                "Sign-in failed",
                "You can close this tab and try again.",
            )
            .await;
            bail!("{err}: {desc}");
        }
        if let Some(code) = q.get("code") {
            if q.get("state").map(String::as_str) != Some(state) {
                respond(
                    &mut sock,
                    "400 Bad Request",
                    "Sign-in failed",
                    "State mismatch.",
                )
                .await;
                bail!("State mismatch – sign-in aborted");
            }
            respond(
                &mut sock,
                "200 OK",
                "Signed in ✓",
                "You can close this tab and return to Sunrise.",
            )
            .await;
            return Ok(code.clone());
        }
        // favicon or other noise
        respond(&mut sock, "404 Not Found", "Not found", "").await;
    }
}

// ───────────────────────── PKCE login ─────────────────────────

pub async fn interactive_login<F>(
    client: &Client,
    client_id: &str,
    open_browser: F,
    cancel: Arc<AtomicBool>,
) -> Result<Account>
where
    F: FnOnce(String) -> Result<()>,
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let redirect = format!("http://localhost:{port}");

    let verifier = random_b64(48);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_b64(16);

    let mut url = Url::parse(AUTH_URL)?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &redirect)
        .append_pair("response_mode", "query")
        .append_pair("scope", SCOPE)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("prompt", "select_account");

    info!(port, "opening browser for Microsoft sign-in");
    open_browser(url.to_string())?;
    let code = wait_for_code(listener, &state, &cancel).await?;

    let v: Value = client
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("code_verifier", verifier.as_str()),
            ("scope", SCOPE),
        ])
        .send()
        .await?
        .json()
        .await?;
    if v.get("error").is_some() {
        bail!(oauth_error(&v));
    }
    let access = s(&v, "access_token")?;
    let refresh = s(&v, "refresh_token")?;
    let session = minecraft_session(client, &access).await?;
    Ok(Account {
        id: 0,
        username: session.name,
        uuid: session.uuid,
        refresh_token: refresh,
        provider: "microsoft".into(),
        api_root: String::new(),
        client_token: String::new(),
        skin_url: session.skin_url.unwrap_or_default(),
    })
}

// ───────────────────────── refresh at launch ─────────────────────────

// Kept for when Microsoft sign-in is re-enabled.
#[allow(dead_code)]
pub async fn refresh(
    client: &Client,
    client_id: &str,
    acc: &Account,
) -> Result<(AuthInfo, Account)> {
    let v: Value = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", acc.refresh_token.as_str()),
            ("scope", SCOPE),
        ])
        .send()
        .await?
        .json()
        .await?;
    if v.get("error").is_some() {
        bail!(
            "Microsoft session expired – sign in again ({})",
            oauth_error(&v)
        );
    }
    let access = s(&v, "access_token")?;
    let new_refresh = v["refresh_token"]
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| acc.refresh_token.clone());
    let session = minecraft_session(client, &access).await?;
    let updated = Account {
        id: acc.id,
        username: session.name.clone(),
        uuid: session.uuid.clone(),
        refresh_token: new_refresh,
        provider: "microsoft".into(),
        api_root: String::new(),
        client_token: String::new(),
        skin_url: session.skin_url.clone().unwrap_or_default(),
    };
    Ok((session, updated))
}

// ───────────────────────── Xbox -> XSTS -> Minecraft ─────────────────────────

async fn minecraft_session(client: &Client, ms_access: &str) -> Result<AuthInfo> {
    // 1) Xbox Live
    let xbl: Value = client
        .post(XBL_URL)
        .header("Accept", "application/json")
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let xbl_token = s(&xbl, "Token")?;
    let uhs = xbl["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .ok_or_else(|| anyhow!("Xbox response had no user hash"))?
        .to_string();

    // 2) XSTS
    let resp = client
        .post(XSTS_URL)
        .header("Accept", "application/json")
        .json(&json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await?;
    if resp.status().as_u16() == 401 {
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        bail!(xsts_error(v["XErr"].as_u64()));
    }
    let xsts: Value = resp.error_for_status()?.json().await?;
    let xsts_token = s(&xsts, "Token")?;

    // 3) Minecraft services
    let resp = client
        .post(MC_LOGIN_URL)
        .json(&json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }))
        .send()
        .await?;
    if resp.status().as_u16() == 403 {
        bail!(
            "Minecraft rejected this app (403). New Azure apps must be approved by Mojang first – \
             see https://aka.ms/mce-reviewappid"
        );
    }
    let mc: Value = resp.error_for_status()?.json().await?;
    let token = s(&mc, "access_token")?;

    // 4) profile (also proves the account owns the game)
    let prof = client
        .get(MC_PROFILE_URL)
        .bearer_auth(&token)
        .send()
        .await?;
    if prof.status().as_u16() == 404 {
        bail!("This Microsoft account doesn't own Minecraft: Java Edition");
    }
    let prof: Value = prof.error_for_status()?.json().await?;
    let skin_url = prof["skins"]
        .as_array()
        .and_then(|skins| skins.first())
        .and_then(|skin| skin["url"].as_str())
        .filter(|url| url.starts_with("https://"))
        .map(String::from);
    Ok(AuthInfo {
        name: s(&prof, "name")?,
        uuid: s(&prof, "id")?,
        token,
        user_type: "msa".into(),
        api_root: None,
        skin_url,
    })
}

/// Authenticate against a preset Yggdrasil server. Passwords and TOTP codes are only
/// used for this request; the launcher stores the returned access token instead.
pub async fn yggdrasil_login(
    client: &Client,
    provider: &str,
    username: &str,
    password: &str,
    totp: &str,
) -> Result<Account> {
    let api_root = yggdrasil_root(provider)?;
    let mut supplied_password = password.to_string();
    if !totp.trim().is_empty() {
        supplied_password.push(':');
        supplied_password.push_str(totp.trim());
    }
    let requested_client_token = random_b64(16);
    let v: Value = client
        .post(format!("{api_root}authserver/authenticate"))
        .json(&json!({
            "agent": { "name": "Minecraft", "version": 1 },
            "username": username.trim(),
            "password": supplied_password,
            "clientToken": requested_client_token,
            "requestUser": true
        }))
        .send()
        .await?
        .json()
        .await?;
    if v.get("error").is_some() {
        bail!(
            "{}",
            v["errorMessage"]
                .as_str()
                .or_else(|| v["error"].as_str())
                .unwrap_or("Sign-in failed")
        );
    }
    let token = s(&v, "accessToken")?;
    let client_token = v["clientToken"]
        .as_str()
        .unwrap_or(&requested_client_token)
        .to_string();
    let profile = v
        .get("selectedProfile")
        .filter(|p| p.is_object())
        .unwrap_or(&v["availableProfiles"][0]);
    let name = s(profile, "name").map_err(|_| anyhow!("This account has no Minecraft profile"))?;
    let uuid = s(profile, "id").map_err(|_| anyhow!("This account has no Minecraft profile"))?;
    let skin_url = profile_skin(client, provider, &uuid)
        .await
        .unwrap_or_default()
        .unwrap_or_default();
    Ok(Account {
        id: 0,
        username: name,
        uuid,
        refresh_token: token,
        provider: provider.into(),
        api_root,
        client_token,
        skin_url,
    })
}

/// Renew a Yggdrasil access token and prepare the launch credentials.
pub async fn refresh_yggdrasil(client: &Client, acc: &Account) -> Result<(AuthInfo, Account)> {
    let api_root = yggdrasil_root(&acc.provider)?;
    if acc.api_root != api_root {
        bail!("Saved authentication server does not match this provider. Sign in again.");
    }
    let v: Value = client
        .post(format!("{api_root}authserver/refresh"))
        .json(&json!({ "accessToken": acc.refresh_token, "clientToken": acc.client_token, "requestUser": true }))
        .send()
        .await?
        .json()
        .await?;
    if v.get("error").is_some() {
        bail!(
            "{} – sign in again",
            v["errorMessage"]
                .as_str()
                .or_else(|| v["error"].as_str())
                .unwrap_or("Session expired")
        );
    }
    let token = v["accessToken"]
        .as_str()
        .unwrap_or(&acc.refresh_token)
        .to_string();
    let profile = v
        .get("selectedProfile")
        .filter(|p| p.is_object())
        .unwrap_or(&v["availableProfiles"][0]);
    let name = profile["name"]
        .as_str()
        .unwrap_or(&acc.username)
        .to_string();
    let uuid = profile["id"].as_str().unwrap_or(&acc.uuid).to_string();
    let skin_url = profile_skin(client, &acc.provider, &uuid)
        .await
        .unwrap_or_default()
        .unwrap_or_else(|| acc.skin_url.clone());
    let updated = Account {
        id: acc.id,
        username: name.clone(),
        uuid: uuid.clone(),
        refresh_token: token.clone(),
        provider: acc.provider.clone(),
        api_root: api_root.clone(),
        client_token: acc.client_token.clone(),
        skin_url: skin_url.clone(),
    };
    let info = AuthInfo {
        name,
        uuid,
        token,
        user_type: "mojang".into(),
        api_root: Some(api_root),
        skin_url: (!skin_url.is_empty()).then_some(skin_url),
    };
    Ok((info, updated))
}

pub fn yggdrasil_root(provider: &str) -> Result<String> {
    let root = match provider {
        "elyby" => "https://authserver.ely.by/api/",
        "littleskin" => "https://littleskin.cn/api/yggdrasil/",
        _ => bail!("Unknown authentication provider"),
    };
    Ok(root.into())
}

/// Resolve a profile's skin texture URL from its Yggdrasil/Mojang profile properties.
pub async fn profile_skin(client: &Client, provider: &str, uuid: &str) -> Result<Option<String>> {
    let root = if provider == "microsoft" {
        MC_SESSION_ROOT.to_string()
    } else {
        yggdrasil_root(provider)?
    };
    let id = uuid.replace('-', "");
    let path = if provider == "microsoft" {
        "session/minecraft/profile/"
    } else {
        "sessionserver/session/minecraft/profile/"
    };
    let request = async {
        let response = client
            .get(format!("{root}{path}{id}?unsigned=false"))
            .send()
            .await?
            .error_for_status()?;
        Ok::<Value, anyhow::Error>(response.json().await?)
    };
    let profile = tokio::time::timeout(Duration::from_secs(6), request)
        .await
        .map_err(|_| anyhow!("Skin profile request timed out"))??;
    let Some(properties) = profile["properties"].as_array() else {
        return Ok(None);
    };
    for property in properties {
        if property["name"].as_str() != Some("textures") {
            continue;
        }
        let Some(value) = property["value"].as_str() else {
            continue;
        };
        let Ok(decoded) = STANDARD.decode(value) else {
            continue;
        };
        let Ok(textures) = serde_json::from_slice::<Value>(&decoded) else {
            continue;
        };
        if let Some(url) = textures["textures"]["SKIN"]["url"]
            .as_str()
            .filter(|url| url.starts_with("https://"))
        {
            return Ok(Some(url.to_string()));
        }
    }
    Ok(None)
}
