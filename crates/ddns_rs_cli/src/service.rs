use std::path::PathBuf;
use std::process::Command;

/// Service management: install/uninstall/restart a system service.
/// Supports systemd (Linux) and launchd (macOS).

#[derive(Debug, Clone)]
pub struct ServiceArgs {
    pub listen: String,
    pub every: u64,
    pub cache_times: i32,
    pub config_file_path: String,
    pub no_web: bool,
    pub skip_verify: bool,
    pub dns: String,
}

impl Default for ServiceArgs {
    fn default() -> Self {
        Self {
            listen: ":9876".to_string(),
            every: 300,
            cache_times: 5,
            config_file_path: String::new(),
            no_web: false,
            skip_verify: false,
            dns: String::new(),
        }
    }
}

fn exe_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ddns-go"))
}

fn is_systemd() -> bool {
    std::path::Path::new("/run/systemd/system").exists()
        || std::env::var("INVOCATION_ID").is_ok()
        || Command::new("systemctl").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

/// Install the service.
pub fn install(args: &ServiceArgs) {
    if is_macos() {
        match install_launchd(args) {
            Ok(_) => ddns_rs_core::log_msg!("安装 ddns-go 服务成功! 请打开浏览器并进行配置"),
            Err(e) => ddns_rs_core::log_msg!("安装 ddns-go 服务失败, 异常信息: %s", e),
        }
    } else if is_systemd() {
        match install_systemd(args) {
            Ok(_) => ddns_rs_core::log_msg!("安装 ddns-go 服务成功! 请打开浏览器并进行配置"),
            Err(e) => ddns_rs_core::log_msg!("安装 ddns-go 服务失败, 异常信息: %s", e),
        }
    } else {
        ddns_rs_core::log_msg!("安装 ddns-go 服务失败, 异常信息: %s", "unsupported service manager");
    }
}

/// Uninstall the service.
pub fn uninstall() {
    if is_macos() {
        match uninstall_launchd() {
            Ok(_) => ddns_rs_core::log_msg!("ddns-go 服务卸载成功"),
            Err(e) => ddns_rs_core::log_msg!("ddns-go 服务卸载失败, 异常信息: %s", e),
        }
    } else if is_systemd() {
        match uninstall_systemd() {
            Ok(_) => ddns_rs_core::log_msg!("ddns-go 服务卸载成功"),
            Err(e) => ddns_rs_core::log_msg!("ddns-go 服务卸载失败, 异常信息: %s", e),
        }
    } else {
        ddns_rs_core::log_msg!("ddns-go 服务卸载失败, 异常信息: %s", "unsupported service manager");
    }
}

/// Restart the service.
pub fn restart() {
    if is_macos() {
        let _ = restart_launchd();
    } else if is_systemd() {
        let _ = restart_systemd();
    } else {
        ddns_rs_core::log_msg!("ddns-go 服务未安装, 请先安装服务");
    }
}

// ---------- systemd ----------

fn install_systemd(args: &ServiceArgs) -> Result<(), String> {
    let exe = exe_path();
    let mut cmd_args = vec![
        "-l".to_string(),
        args.listen.clone(),
        "-f".to_string(),
        args.every.to_string(),
        "-cacheTimes".to_string(),
        args.cache_times.to_string(),
        "-c".to_string(),
        args.config_file_path.clone(),
    ];
    if args.no_web {
        cmd_args.push("-noweb".to_string());
    }
    if args.skip_verify {
        cmd_args.push("-skipVerify".to_string());
    }
    if !args.dns.is_empty() {
        cmd_args.push("-dns".to_string());
        cmd_args.push(args.dns.clone());
    }

    let arg_str = cmd_args
        .iter()
        .map(|a| format!("{}", shell_quote(a)))
        .collect::<Vec<_>>()
        .join(" ");

    let unit = format!(
        "[Unit]\n\
         Description=Simple and easy to use DDNS\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={} {}\n\
         Restart=on-failure\n\
         RestartSec=5s\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        exe.display(),
        arg_str
    );

    let unit_path = PathBuf::from("/etc/systemd/system/ddns-go.service");
    std::fs::write(&unit_path, unit).map_err(|e| e.to_string())?;

    run_cmd("systemctl", &["daemon-reload"])?;
    run_cmd("systemctl", &["enable", "ddns-go.service"])?;
    run_cmd("systemctl", &["start", "ddns-go.service"])?;
    Ok(())
}

fn uninstall_systemd() -> Result<(), String> {
    let _ = run_cmd("systemctl", &["stop", "ddns-go.service"]);
    let _ = run_cmd("systemctl", &["disable", "ddns-go.service"]);
    std::fs::remove_file("/etc/systemd/system/ddns-go.service").ok();
    run_cmd("systemctl", &["daemon-reload"])?;
    Ok(())
}

fn restart_systemd() -> Result<(), String> {
    run_cmd("systemctl", &["restart", "ddns-go.service"])
}

// ---------- launchd (macOS) ----------

fn launchd_plist_path() -> PathBuf {
    PathBuf::from(format!(
        "{}/Library/LaunchAgents/com.ddnsgo.ddns-go.plist",
        std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
    ))
}

fn install_launchd(args: &ServiceArgs) -> Result<(), String> {
    let exe = exe_path();
    let mut program_args = vec![
        "-l".to_string(),
        args.listen.clone(),
        "-f".to_string(),
        args.every.to_string(),
        "-cacheTimes".to_string(),
        args.cache_times.to_string(),
        "-c".to_string(),
        args.config_file_path.clone(),
    ];
    if args.no_web {
        program_args.push("-noweb".to_string());
    }
    if args.skip_verify {
        program_args.push("-skipVerify".to_string());
    }
    if !args.dns.is_empty() {
        program_args.push("-dns".to_string());
        program_args.push(args.dns.clone());
    }

    let args_xml: Vec<String> = program_args
        .iter()
        .map(|a| format!("        <string>{}</string>", xml_escape(a)))
        .collect();
    let args_xml = args_xml.join("\n");

    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
             <key>Label</key>\n\
             <string>com.ddnsgo.ddns-go</string>\n\
             <key>ProgramArguments</key>\n\
             <array>\n\
                 <string>{}</string>\n\
         {}\n\
             </array>\n\
             <key>RunAtLoad</key>\n\
             <true/>\n\
             <key>KeepAlive</key>\n\
             <true/>\n\
             <key>StandardOutPath</key>\n\
             <string>/tmp/ddns-go.stdout.log</string>\n\
             <key>StandardErrorPath</key>\n\
             <string>/tmp/ddns-go.stderr.log</string>\n\
         </dict>\n\
         </plist>\n",
        exe.display(),
        args_xml
    );

    let path = launchd_plist_path();
    std::fs::write(&path, plist).map_err(|e| e.to_string())?;

    let _ = run_cmd("launchctl", &["unload", path.to_str().unwrap()]);
    run_cmd("launchctl", &["load", path.to_str().unwrap()])?;
    Ok(())
}

fn uninstall_launchd() -> Result<(), String> {
    let path = launchd_plist_path();
    let _ = run_cmd("launchctl", &["unload", path.to_str().unwrap()]);
    std::fs::remove_file(&path).ok();
    Ok(())
}

fn restart_launchd() -> Result<(), String> {
    let path = launchd_plist_path();
    let _ = run_cmd("launchctl", &["unload", path.to_str().unwrap()]);
    run_cmd("launchctl", &["load", path.to_str().unwrap()])
}

// ---------- helpers ----------

fn run_cmd(program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run {}: {}", program, e))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} {} failed: {}",
            program,
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn shell_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
