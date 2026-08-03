use serde::Deserialize;

/// GitHub repo for releases, overridable via DDNS_RS_REPO env (default: jeessy2/ddns-rs).
fn github_api() -> String {
    let repo = std::env::var("DDNS_RS_REPO").unwrap_or_else(|_| "jeessy2/ddns-rs".to_string());
    format!("https://api.github.com/repos/{}/releases/latest", repo)
}

/// Run an async future to completion in a temporary blocking runtime.
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(fut)
}

#[derive(Deserialize)]
struct ReleaseResp {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Self-update: check GitHub latest release and replace the binary.
pub fn self_update(current_version: &str) {
    let current = match semver::Version::parse(current_version) {
        Ok(v) => v,
        Err(_) => {
            println!("Cannot update because current version {} is not a semver", current_version);
            return;
        }
    };

    let (name, url, latest) = match detect_latest() {
        Ok(Some(l)) => l,
        Ok(None) => {
            println!("Cannot find any release for this OS/arch");
            return;
        }
        Err(e) => {
            println!("Error happened when detecting latest version: {}", e);
            return;
        }
    };

    if current >= latest {
        println!("Current version ({}) is the latest", current_version);
        return;
    }

    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            println!("Cannot find executable path: {}", e);
            return;
        }
    };

    if let Err(e) = download_and_replace(&url, &name, &exe) {
        println!("Error happened when updating binary: {}", e);
        return;
    }

    println!("Success update to v{}", latest);
}

fn detect_latest() -> Result<Option<(String, String, semver::Version)>, String> {
    let client = ddns_rs_core::httpclient::create_http_client();
    let result = block_on(async {
        let resp = client
            .get(github_api())
            .header("User-Agent", "ddns-rs")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("GitHub API returned status {}", resp.status()));
        }
        let release: ReleaseResp = resp.json().await.map_err(|e| e.to_string())?;

        let version = semver::Version::parse(release.tag_name.trim_start_matches('v'))
            .map_err(|e| format!("cannot parse version {}: {}", release.tag_name, e))?;

        let os = std::env::consts::OS;
        let arch = match std::env::consts::ARCH {
            "x86_64" => "x86_64".to_string(),
            "aarch64" => "arm64".to_string(),
            "arm" => "armv7".to_string(),
            other => other.to_string(),
        };
        let zip_ext = if os == "windows" { ".zip" } else { ".tar.gz" };

        for asset in &release.assets {
            let suffix = format!("{}_{}{}", os, arch, zip_ext);
            if asset.name.ends_with(&suffix) {
                return Ok(Some((asset.name.clone(), asset.browser_download_url.clone(), version)));
            }
        }
        Ok(None)
    });
    result
}

fn download_and_replace(url: &str, asset_name: &str, exe_path: &std::path::Path) -> Result<(), String> {
    let client = ddns_rs_core::httpclient::create_http_client();
    let bytes = block_on(async {
        let resp = client
            .get(url)
            .header("User-Agent", "ddns-rs")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if resp.status().as_u16() >= 300 {
            return Err(format!(
                "could not download release from {}. Response code: {}",
                url,
                resp.status()
            ));
        }
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        Ok::<Vec<u8>, String>(bytes.to_vec())
    })?;

    // Extract the binary
    let temp_dir = std::env::temp_dir().join(format!("ddns-rs-update-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    let archive_path = temp_dir.join(asset_name);
    std::fs::write(&archive_path, &bytes).map_err(|e| e.to_string())?;

    let binary = if asset_name.ends_with(".zip") {
        extract_zip(&archive_path, &temp_dir)?
    } else {
        extract_tar_gz(&archive_path, &temp_dir)?
    };

    // Replace the running binary (keep a backup)
    let exe_str = exe_path.to_string_lossy().to_string();
    let backup = exe_path.with_extension("old");
    let _ = std::fs::remove_file(&backup);
    if backup.exists() {
        let _ = std::fs::remove_file(&backup);
    }
    if let Ok(meta) = std::fs::metadata(&exe_str) {
        let _ = meta;
    }

    // Write to a temp name first, then rename over (atomic-ish on Unix)
    let new_path = exe_path.with_extension("new");
    std::fs::copy(&binary, &new_path).map_err(|e| e.to_string())?;
    make_executable(&new_path)?;
    std::fs::rename(&new_path, &exe_path).map_err(|e| e.to_string())?;

    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(())
}

fn extract_tar_gz(archive: &std::path::Path, dest: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);
    tar.unpack(dest).map_err(|e| e.to_string())?;
    find_binary(dest)
}

fn extract_zip(archive: &std::path::Path, dest: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    zip.extract(dest).map_err(|e| e.to_string())?;
    find_binary(dest)
}

fn find_binary(dest: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let entries = std::fs::read_dir(dest).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            if name == "ddns-rs" || name == "ddns-rs.exe" {
                return Ok(path);
            }
        }
        // Check subdirectories (some archives nest)
        if path.is_dir() {
            if let Ok(bin) = find_binary(&path) {
                return Ok(bin);
            }
        }
    }
    Err("cannot find ddns-rs binary in archive".to_string())
}

fn make_executable(path: &std::path::Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
    }
    Ok(())
}
