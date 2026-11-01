//! Linux GPU detection + PRIME render offload environment.

use serde::Serialize;

#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)] // Intel/Other are only constructed on Linux
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub vendor: Vendor,
    pub card: String,
    /// the GPU the system booted on (usually the integrated one on laptops)
    pub boot_vga: bool,
}

#[cfg(target_os = "linux")]
pub fn detect() -> Vec<Gpu> {
    use std::fs;
    let mut gpus = Vec::new();
    let Ok(rd) = fs::read_dir("/sys/class/drm") else {
        return gpus;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let dev = e.path().join("device");
        let Ok(id) = fs::read_to_string(dev.join("vendor")) else {
            continue;
        };
        let vendor = match id.trim() {
            "0x10de" => Vendor::Nvidia,
            "0x1002" => Vendor::Amd,
            "0x8086" => Vendor::Intel,
            _ => Vendor::Other,
        };
        let boot_vga = fs::read_to_string(dev.join("boot_vga"))
            .map(|s| s.trim() == "1")
            .unwrap_or(false);
        gpus.push(Gpu {
            vendor,
            card: name,
            boot_vga,
        });
    }
    gpus.sort_by(|a, b| a.card.cmp(&b.card));
    gpus
}

#[cfg(not(target_os = "linux"))]
pub fn detect() -> Vec<Gpu> {
    Vec::new()
}

/// Environment variables that move rendering to the dedicated GPU.
/// Empty when there is only one GPU (nothing to offload to).
pub fn offload_env(gpus: &[Gpu]) -> Vec<(&'static str, &'static str)> {
    let mut env = Vec::new();
    if gpus.len() < 2 {
        return env;
    }
    if gpus.iter().any(|g| g.vendor == Vendor::Nvidia) {
        env.push(("__NV_PRIME_RENDER_OFFLOAD", "1"));
        env.push(("__GLX_VENDOR_LIBRARY_NAME", "nvidia"));
        env.push(("__VK_LAYER_NV_optimus", "NVIDIA_only"));
    }
    if gpus.iter().any(|g| g.vendor == Vendor::Amd) {
        env.push(("DRI_PRIME", "1"));
    }
    env
}
