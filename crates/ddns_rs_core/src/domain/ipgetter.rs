use crate::config::DnsConfig;

/// Get IPv4 address based on get type.
pub async fn get_ipv4_addr(dns_conf: &DnsConfig) -> String {
    match dns_conf.Ipv4.GetType.as_str() {
        "netInterface" => get_ipv4_from_interface(&dns_conf.Ipv4.NetInterface),
        "url" => get_ip_from_url(&dns_conf.Ipv4.URL, true, &dns_conf.HttpInterface).await,
        "cmd" => get_addr_from_cmd(&dns_conf.Ipv4.Cmd, true),
        _ => String::new(),
    }
}

/// Get IPv6 address based on get type.
pub async fn get_ipv6_addr(dns_conf: &DnsConfig) -> String {
    match dns_conf.Ipv6.GetType.as_str() {
        "netInterface" => {
            get_ipv6_from_interface(&dns_conf.Ipv6.NetInterface, &dns_conf.Ipv6.Ipv6Reg)
        }
        "url" => get_ip_from_url(&dns_conf.Ipv6.URL, false, &dns_conf.HttpInterface).await,
        "cmd" => get_addr_from_cmd(&dns_conf.Ipv6.Cmd, false),
        _ => String::new(),
    }
}

fn get_ipv4_from_interface(iface_name: &str) -> String {
    if let Ok((ipv4, _)) = crate::netiface::get_net_interface() {
        for iface in ipv4 {
            if iface.name == iface_name && !iface.address.is_empty() {
                return iface.address[0].clone();
            }
        }
    }
    crate::log_msg!("从网卡中获得IPv4失败! 网卡名: %s", iface_name);
    String::new()
}

fn get_ipv6_from_interface(iface_name: &str, ipv6_reg: &str) -> String {
    if let Ok((_, ipv6)) = crate::netiface::get_net_interface() {
        for iface in ipv6 {
            if iface.name == iface_name && !iface.address.is_empty() {
                if !ipv6_reg.is_empty() {
                    // @\d means pick the Nth IPv6 address
                    if ipv6_reg.starts_with('@') {
                        if let Ok(num) = ipv6_reg[1..].parse::<usize>() {
                            if num > 0 {
                                if num <= iface.address.len() {
                                    return iface.address[num - 1].clone();
                                }
                                crate::log_msg!(
                                    "未找到第 %d 个IPv6地址! 将使用第一个IPv6地址",
                                    num
                                );
                                return iface.address[0].clone();
                            }
                            crate::log_msg!("IPv6匹配表达式 %s 不正确! 最小从1开始", ipv6_reg);
                            return String::new();
                        }
                    }
                    // regex match
                    crate::log_msg!("IPv6将使用正则表达式 %s 进行匹配", ipv6_reg);
                    if let Ok(re) = regex::Regex::new(ipv6_reg) {
                        for addr in &iface.address {
                            if re.is_match(addr) {
                                crate::log_msg!("匹配成功! 匹配到地址: %s", addr);
                                return addr.clone();
                            }
                        }
                    }
                    crate::log_msg!("没有匹配到任何一个IPv6地址, 将使用第一个地址");
                }
                return iface.address[0].clone();
            }
        }
    }
    crate::log_msg!("从网卡中获得IPv6失败! 网卡名: %s", iface_name);
    String::new()
}

async fn get_ip_from_url(urls: &str, ipv4: bool, http_interface: &str) -> String {
    let client = crate::httpclient::bound_client(http_interface, if ipv4 { "tcp4" } else { "tcp6" });
    for url in urls.split(',') {
        let url = url.trim();
        if url.is_empty() {
            continue;
        }
        match client.get(url).send().await {
            Ok(resp) => {
                let body = resp.text().await.unwrap_or_default();
                if let Some(result) = extract_ip(&body, ipv4) {
                    return result;
                }
                crate::log_msg!(
                    "获取IPv{}结果失败! 接口: {} ,返回值: {}",
                    if ipv4 { 4 } else { 6 },
                    url,
                    body
                );
            }
            Err(e) => {
                crate::log_msg!(
                    "通过接口获取IPv{}失败! 接口地址: {}",
                    if ipv4 { 4 } else { 6 },
                    url
                );
                crate::log_msg!("异常信息: %s", e);
            }
        }
    }
    String::new()
}

fn get_addr_from_cmd(cmd: &str, ipv4: bool) -> String {
    if cmd.is_empty() {
        return String::new();
    }
    let shell = if cfg!(windows) {
        "powershell"
    } else {
        "sh"
    };
    let output = if cfg!(windows) {
        std::process::Command::new(shell)
            .arg("-Command")
            .arg(cmd)
            .output()
    } else {
        std::process::Command::new(shell)
            .arg("-c")
            .arg(cmd)
            .output()
    };
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let full = if out.stderr.is_empty() {
                stdout
            } else {
                format!(
                    "{}{}",
                    stdout,
                    String::from_utf8_lossy(&out.stderr)
                )
            };
            if !out.status.success() {
                crate::log_msg!(
                    "获取{}结果失败! 未能成功执行命令：%s, 错误：%q, 退出状态码：%s",
                    if ipv4 { "IPv4" } else { "IPv6" },
                    cmd,
                    full,
                    out.status.code().map(|c| c.to_string()).unwrap_or_default()
                );
                return String::new();
            }
            if let Some(result) = extract_ip(&full, ipv4) {
                return result;
            }
            crate::log_msg!(
                "获取{}结果失败! 命令: %s, 标准输出: %q",
                if ipv4 { "IPv4" } else { "IPv6" },
                cmd,
                full
            );
            String::new()
        }
        Err(e) => {
            crate::log_msg!(
                "获取{}结果失败! 未能成功执行命令：%s, 错误：%q, 退出状态码：%s",
                if ipv4 { "IPv4" } else { "IPv6" },
                cmd,
                e.to_string(),
                "error"
            );
            String::new()
        }
    }
}

/// Extract IPv4/IPv6 from text.
pub fn extract_ip(text: &str, ipv4: bool) -> Option<String> {
    if ipv4 {
        crate::iputil::IPV4_REG.find(text).map(|m| m.as_str().to_string())
    } else {
        crate::iputil::find_ipv6(text)
    }
}
