// vite-luci.mjs - Vite plugin for the ddns-rs LuCI frontend.
//
// `npm run build` (vite build) copies the SPA bundle into the LuCI package
// htdocs and, by default, directly produces the .ipk/.apk packages by
// invoking build-luci-package.sh. So building the frontend yields the
// distributable OpenWrt packages in one step.
//
//   npm run build              # bundle + build ipk/apk into ../dist
//   npm run build -- --no-pkg  # bundle only, skip packaging
//   npm run build -- --check   # only verify the bundle exists in the package
//
// The package version is read from package.json (must match the release).

import { readdirSync, copyFileSync, mkdirSync, rmSync, existsSync, readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { spawnSync } from 'node:child_process'

const __dirname = dirname(fileURLToPath(import.meta.url))
const root = __dirname
const distDir = join(root, 'vite-app', 'dist')
const outDir = join(root, 'luci-app-ddns-rs', 'htdocs', 'luci-static', 'resources', 'ddns-rs-app')
const pkgJson = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))

const BUNDLE = 'ddns-rs-app.js'
const CSS = 'ddns-rs-app.css'

export default function luciPlugin() {
  return {
    name: 'luci-package',
    apply: 'build',

    closeBundle() {
      if (process.env.LUCI_CHECK === '1') {
        runCheck()
        return
      }

      copyBundle()
      console.log(`[luci-package] bundle copied to ${outDir}`)

      if (process.env.LUCI_PKG === '0' || process.env.LUCI_PKG === 'no')
        return

      packageLuci()
    }
  }
}

function copyBundle() {
  if (!existsSync(distDir))
    throw new Error('dist directory not found - vite build did not produce output')

  rmSync(outDir, { recursive: true, force: true })
  mkdirSync(outDir, { recursive: true })

  for (const file of readdirSync(distDir)) {
    if (file === BUNDLE || file === CSS)
      copyFileSync(join(distDir, file), join(outDir, file))
  }
}

function packageLuci() {
  const version = pkgJson.version
  const script = join(root, 'scripts', 'build-luci-package.sh')
  const outRoot = join(root, '..', 'dist')

  console.log(`[luci-package] building ipk/apk ${version} ...`)
  const res = spawnSync('sh', [script, version, outRoot], {
    cwd: join(root, '..'),  // repository root, script paths are repo-relative
    stdio: 'inherit'
  })

  if (res.status !== 0)
    process.exit(res.status ?? 1)

  console.log(`[luci-package] packages written to ${outRoot}`)
}

function runCheck() {
  const missing = []
  if (!existsSync(join(outDir, BUNDLE)))
    missing.push(BUNDLE)
  if (missing.length) {
    console.error(`[luci-package] check failed, missing: ${missing.join(', ')}`)
    process.exit(1)
  }
  console.log('[luci-package] check ok')
}
