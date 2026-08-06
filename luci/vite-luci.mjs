// vite-luci.mjs - Vite plugin for the ddns-rs LuCI frontend.
//
// On `vite build` (via `npm run build`) it copies the SPA bundle
// (ddns-rs-app.js / .css) into the LuCI package's htdocs so the existing
// build-luci-package.sh produces the ipk/apk containing the Vue app.
//
// The LuCI page (binary.html view) loads the bundle in place of the old
// hand-written binary.js.

import { readdirSync, copyFileSync, mkdirSync, rmSync, existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))
const root = __dirname
const distDir = join(root, 'vite-app', 'dist')
const outDir = join(root, 'luci-app-ddns-rs', 'htdocs', 'luci-static', 'resources', 'ddns-rs-app')

const BUNDLE = 'ddns-rs-app.js'
const CSS = 'ddns-rs-app.css'

export default function luciPlugin() {
  return {
    name: 'luci-package',
    apply: 'build',

    closeBundle() {
      if (process.argv.includes('--check')) {
        runCheck()
        return
      }

      if (!existsSync(distDir))
        throw new Error('dist directory not found - vite build did not produce output')

      // copy built bundle into the LuCI package htdocs
      rmSync(outDir, { recursive: true, force: true })
      mkdirSync(outDir, { recursive: true })

      for (const file of readdirSync(distDir)) {
        if (file === BUNDLE || file === CSS)
          copyFileSync(join(distDir, file), join(outDir, file))
      }

      console.log(`[luci-package] bundle copied to ${outDir}`)
    }
  }
}

function runCheck() {
  const missing = []
  if (!existsSync(join(outDir, BUNDLE)))
    missing.push(BUNDLE)
  if (!existsSync(join(outDir, CSS)))
    missing.push(CSS)
  if (missing.length) {
    console.error(`[luci-package] check failed, missing: ${missing.join(', ')}`)
    process.exit(1)
  }
  console.log('[luci-package] check ok')
}
