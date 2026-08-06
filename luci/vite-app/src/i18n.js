// i18n.js - minimal translation helper.
// Tries LuCI's global _() first (page loaded inside LuCI provides it),
// falls back to a small built-in map for standalone/dev usage.

const builtin = {
  // binary page
  'DDNS-RS Binary Status': 'DDNS-RS 二进制状态',
  'Status check error': '状态检查错误',
  'Installed': '已安装',
  'Yes': '是',
  'No': '否',
  'Not installed': '未安装',
  'Version': '版本',
  'Binary path': '二进制路径',
  'Service': '服务',
  'Running': '运行中',
  'Stopped': '已停止',
  'Install Binary': '安装二进制',
  'Upload & Install': '上传并安装',
  'Install from URL': '从 URL 安装',
  'Auto Install/Update': '自动安装/更新',
  'Auto Install / Update': '自动安装 / 更新',
  'Working...': '工作中...',
  'Operation completed.': '操作完成。',
  'Operation failed': '操作失败',
  'Please enter a download URL': '请输入下载地址',
  'Enter a direct download URL of the ddns-rs binary or .tar.gz archive.': '请输入 ddns-rs 二进制或 .tar.gz 压缩包的直链下载地址。',
  'If the binary is missing, it will be downloaded from the default release. If installed, it will check for the latest version and update automatically.': '若缺少二进制将自动从默认发行版下载；若已安装则检查最新版本并自动更新。',

  // base setting page
  'RUNNING': '运行中',
  'NOT RUNNING': '未运行',
  'Open Web Interface': '打开Web界面',
  'Default web interface login: Username: admin, Password: admin12345': 'Web 界面默认登录：用户名 admin，密码 admin12345',
  'Base Setting': '基础设置',
  'Enable': '启用',
  'Listen address': '监听地址',
  'Full listen address, e.g. [::]:9876 or 0.0.0.0:9876': '完整监听地址，例如 [::]:9876 或 0.0.0.0:9876',
  'Update interval (seconds)': '更新间隔（秒）',
  'Update interval in seconds (60-86400)': '更新间隔范围(60-86400)秒',
  'Provider comparison interval': '提供商比较间隔',
  'Number of times to compare with service provider (1-60)': '与服务提供商间隔比较的次数（1-60）',
  'Skip verifying certificates': '跳过证书验证',
  'Specify DNS resolution server': '指定DNS解析服务器',
  'Do not start web services': '不启动Web服务',
  'Delayed Start (seconds)': '延迟启动（秒）',
  'Save & Apply': '保存并应用',
  'Reset': '重置',
  'Reset account password': '重置账户密码',
  'Saved. Restart the service to apply.': '已保存，重启服务后生效。',
  'Reset web interface password to admin/admin12345?': '将 Web 界面密码重置为 admin/admin12345？',
  'Password reset. Restart the service to apply.': '密码已重置，重启服务后生效。',

  // log page
  'Loading logs...': '正在加载日志...',
  'No ddns-rs logs found.': '未找到 ddns-rs 日志。',
  'Clearing...': '正在清除...',
  'Clear Logs': '清除日志',
  'Refresh': '刷新',
  'Refresh every 5 seconds.': '每 5 秒刷新',
  'Failed to read logs: %s': '读取日志失败: %s'
}

export function useI18n() {
  const translate = (key) => {
    if (window.L && typeof window.L._ === 'function') {
      const s = window.L._(key)
      if (s && s !== key)
        return s
    }
    return builtin[key] || key
  }
  return { t: translate }
}
