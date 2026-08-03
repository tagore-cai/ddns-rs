use clap::Parser;
use ddns_rs_core::{config, httpclient, logger};

/// ddns-go: Simple and easy to use DDNS.
#[derive(Parser, Debug)]
#[command(name = "ddns-rs", version = ddns_rs_core::VERSION, about = "Simple and easy to use DDNS. Automatically update domain name resolution to public IP", disable_version_flag = true)]
struct Cli {
    /// Show ddns-go version
    #[arg(short = 'v', long)]
    version: bool,

    /// Upgrade ddns-go to the latest version
    #[arg(short = 'u', long)]
    update: bool,

    /// Listen address
    #[arg(short = 'l', default_value = ":9876")]
    listen: String,

    /// Update frequency (seconds)
    #[arg(short = 'f', default_value_t = 300)]
    every: u64,

    /// Cache times
    #[arg(long = "cacheTimes", default_value_t = 5)]
    cache_times: i32,

    /// Service management (install|uninstall|restart)
    #[arg(short = 's', default_value = "")]
    service: String,

    /// Custom configuration file path
    #[arg(short = 'c', default_value = "")]
    config_file_path: String,

    /// No web service
    #[arg(long = "noweb")]
    no_web: bool,

    /// Skip certificate verification
    #[arg(long = "skipVerify")]
    skip_verify: bool,

    /// Custom DNS server address, example: 8.8.8.8
    #[arg(long = "dns", default_value = "")]
    dns: String,

    /// Reset password to the one entered
    #[arg(long = "resetPassword", default_value = "")]
    reset_password: String,

    /// Run in background (daemon/detached)
    #[arg(short = 'd', long)]
    daemon: bool,
}

fn main() -> anyhow::Result<()> {
    // Go flag 包允许单横线长选项(如 -resetPassword), 转换到 clap 的双横线格式。
    let args: Vec<String> = std::env::args()
        .enumerate()
        .map(|(i, a)| {
            if i > 0
                && a.starts_with('-')
                && !a.starts_with("--")
                && a.len() > 2
                && !a.chars().nth(1).is_none()
                && !a[1..].chars().all(|c| c.is_ascii_digit())
            {
                format!("--{}", &a[1..])
            } else {
                a
            }
        })
        .collect();
    let cli = Cli::parse_from(&args);

    if cli.version {
        println!("{}", ddns_rs_core::VERSION);
        return Ok(());
    }
    if cli.update {
        #[cfg(feature = "self-update")]
        ddns_rs_cli::update::self_update(ddns_rs_core::VERSION);
        #[cfg(not(feature = "self-update"))]
        println!("Self-update is not compiled in this build (feature 'self-update' disabled)");
        return Ok(());
    }

    // Set config file path env
    if !cli.config_file_path.is_empty() {
        let abs = std::path::absolute(&cli.config_file_path)
            .unwrap_or_else(|_| cli.config_file_path.clone().into());
        std::env::set_var(config::CONFIG_FILE_PATH_ENV, abs);
    }

    // Reset password
    if !cli.reset_password.is_empty() {
        config::reset_password(&cli.reset_password);
        return Ok(());
    }

    // Daemonize
    if cli.daemon && std::env::var("DDNS_GO_DAEMON").as_deref() != Ok("1") {
        match run_as_daemon(&cli) {
            Ok(_) => {
                logger::log_line("daemonized");
            }
            Err(e) => {
                logger::log_line(&format!("Daemonize failed: {}", e));
                std::process::exit(1);
            }
        }
    } else {
        // skip verify
        if cli.skip_verify {
            httpclient::set_insecure_skip_verify();
        }
        // custom DNS
        if !cli.dns.is_empty() {
            httpclient::set_dns(&cli.dns);
        }
        std::env::set_var(config::IP_CACHE_TIMES_ENV, cli.cache_times.to_string());
    }

    match cli.service.as_str() {
        "install" => {
            let args = build_service_args(&cli);
            ddns_rs_cli::service::install(&args);
        }
        "uninstall" => {
            ddns_rs_cli::service::uninstall();
        }
        "restart" => {
            ddns_rs_cli::service::restart();
        }
        _ => {
            run(cli);
        }
    }
    Ok(())
}

fn build_service_args(cli: &Cli) -> ddns_rs_cli::service::ServiceArgs {
    ddns_rs_cli::service::ServiceArgs {
        listen: cli.listen.clone(),
        every: cli.every,
        cache_times: cli.cache_times,
        config_file_path: cli.config_file_path.clone(),
        no_web: cli.no_web,
        skip_verify: cli.skip_verify,
        dns: cli.dns.clone(),
    }
}

/// Run the process detached from the terminal (setsid on Unix).
fn run_as_daemon(cli: &Cli) -> anyhow::Result<()> {
    let _args: Vec<String> = std::env::args()
        .filter(|a| a != "-d" && a != "--d" && a != "--daemon" && a != "-daemon")
        .collect();

    use daemonize::Daemonize;
    let daemon = Daemonize::new()
        .working_directory(std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")))
        .umask(0o027);
    daemon.start().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    std::env::set_var("DDNS_GO_DAEMON", "1");
    let _ = cli;
    Ok(())
}

fn run(cli: Cli) {
    // Initialize language from config and run compatibility migrations
    if let Ok(mut conf) = config::get_config_cached() {
        logger::init_lang(&conf.Lang);
        config::compatible_config(&mut conf);
    }

    let every = cli.every;
    let listen = cli.listen.clone();
    let no_web = cli.no_web;

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Web server
    if !no_web {
        let listen_clone = listen.clone();
        rt.spawn(async move {
            if let Err(e) = ddns_rs_web::run(&listen_clone).await {
                logger::log_line(&format!("监听端口发生异常, 请检查端口是否被占用! {}", e));
                std::process::exit(1);
            }
        });
    }

    // DNS timer
    let factory: &'static ddns_rs_providers::engine::ProviderFactory = &ddns_rs_providers::PROVIDER_FACTORY;
    rt.spawn(async move {
        ddns_rs_providers::engine::run_timer(factory, std::time::Duration::from_secs(every)).await;
    });

    // Block until SIGINT/SIGTERM (Ctrl-C / system stop), then exit gracefully.
    rt.block_on(async {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigint = signal(SignalKind::interrupt()).unwrap();
            let mut sigterm = signal(SignalKind::terminate()).unwrap();
            tokio::select! {
                _ = sigint.recv() => logger::log_line("Received SIGINT, shutting down"),
                _ = sigterm.recv() => logger::log_line("Received SIGTERM, shutting down"),
            }
        }
        #[cfg(not(unix))]
        {
            match tokio::signal::ctrl_c().await {
                Ok(()) => logger::log_line("Received Ctrl-C, shutting down"),
                Err(e) => logger::log_line(&format!("Failed to listen for Ctrl-C: {}", e)),
            }
        }
    });
}
