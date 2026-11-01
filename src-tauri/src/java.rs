//! Java discovery (async: probing runs `java -version`).

use directories::BaseDirs;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Executable name of the Java launcher.
const JAVA_BIN: &str = if cfg!(windows) { "java.exe" } else { "java" };

#[derive(Clone, Debug, Serialize)]
pub struct JavaInfo {
    pub path: String,
    pub major: u32,
}

/// Every usable Java we can find (JAVA_HOME, PATH, /usr/lib/jvm, sdkman, user-added…).
pub async fn find_all(extra: &[String]) -> Vec<JavaInfo> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    for e in extra {
        let p = PathBuf::from(e);
        candidates.push(if p.is_dir() {
            p.join("bin").join(JAVA_BIN)
        } else {
            p
        });
    }
    if let Ok(home) = std::env::var("JAVA_HOME") {
        candidates.push(PathBuf::from(home).join("bin").join(JAVA_BIN));
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            candidates.push(dir.join(JAVA_BIN));
        }
    }

    let mut roots: Vec<PathBuf> = vec![
        "/usr/lib/jvm".into(),
        "/usr/lib64/jvm".into(),
        "/opt/java".into(),
        "/opt/jdk".into(),
        "/Library/Java/JavaVirtualMachines".into(),
    ];
    if let Some(b) = BaseDirs::new() {
        roots.push(b.home_dir().join(".sdkman/candidates/java"));
        roots.push(b.home_dir().join(".jdks"));
    }
    #[cfg(windows)]
    {
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(pf) = std::env::var_os(var) {
                let pf = PathBuf::from(pf);
                for vendor in [
                    "Java",
                    "Eclipse Adoptium",
                    "Eclipse Foundation",
                    "Microsoft",
                    "Zulu",
                    "BellSoft",
                    "Amazon Corretto",
                    "Semeru",
                    "AdoptOpenJDK",
                ] {
                    roots.push(pf.join(vendor));
                }
            }
        }
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            roots.push(PathBuf::from(l).join("Programs").join("Eclipse Adoptium"));
        }
    }
    for root in roots {
        if let Ok(rd) = std::fs::read_dir(root) {
            for e in rd.flatten() {
                candidates.push(e.path().join("bin").join(JAVA_BIN));
                candidates.push(e.path().join("Contents/Home/bin").join(JAVA_BIN));
            }
        }
    }

    let mut found: Vec<JavaInfo> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    for c in candidates {
        if !c.is_file() {
            continue;
        }
        let real = std::fs::canonicalize(&c).unwrap_or_else(|_| c.clone());
        if seen.contains(&real) {
            continue;
        }
        seen.push(real);
        if let Some(info) = probe(&c).await {
            found.push(info);
        }
    }
    found.sort_by(|a, b| b.major.cmp(&a.major));
    found
}

pub async fn probe(path: &Path) -> Option<JavaInfo> {
    let mut cmd = Command::new(path);
    cmd.arg("-version");
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let out = cmd.output().await.ok()?;
    let text = String::from_utf8_lossy(&out.stderr).to_string();
    let start = text.find('"')? + 1;
    let end = text[start..].find('"')? + start;
    let version = &text[start..end]; // "17.0.9", "1.8.0_392", "21-ea"
    let mut parts = version.split(|c| c == '.' || c == '_' || c == '-' || c == '+');
    let first: u32 = parts.next()?.parse().ok()?;
    let major = if first == 1 {
        parts.next()?.parse().ok()?
    } else {
        first
    };
    Some(JavaInfo {
        path: path.to_string_lossy().to_string(),
        major,
    })
}

/// Accepts the `java` binary or an install folder (e.g. /opt/jdk-21).
pub async fn probe_custom(raw: &str) -> Option<JavaInfo> {
    let p = PathBuf::from(raw.trim());
    let exe = if p.is_dir() {
        p.join("bin").join(JAVA_BIN)
    } else {
        p
    };
    if !exe.is_file() {
        return None;
    }
    probe(&exe).await
}

/// Smallest installed Java that satisfies the requirement (old Minecraft dislikes very new Java).
pub fn pick(javas: &[JavaInfo], required: u32) -> Option<&JavaInfo> {
    javas
        .iter()
        .filter(|j| j.major >= required)
        .min_by_key(|j| j.major)
}
