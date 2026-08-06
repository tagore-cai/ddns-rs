// i18n.js - minimal translation helper.
// Tries LuCI's global _() first (page loaded inside LuCI provides it),
// falls back to a small built-in map for standalone/dev usage.

const builtin = {
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
  'Enter a direct download URL of the ddns-rs binary or .tar.gz archive.': '请输入 ddns-rs 二进制或 .tar.gz 压缩包的直链下载地址。'
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
