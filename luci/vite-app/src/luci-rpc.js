// luci-rpc.js - access LuCI's native rpc/uci/fs clients from the Vue SPA.
//
// The Vue app is loaded inside a LuCI page (window.L available after the
// page initializes). Instead of re-implementing the ubus JSON-RPC client
// (session/CSRF/path probing), we reuse LuCI's own modules via L.require,
// exactly as LuCI's own init does:
//
//   Promise.all([domReady, require('ui'), require('rpc'), require('form')])
//
// Returns a Promise that resolves to { rpc, uci, fs } once LuCI is ready.

let cached = null

// Resolve once the DOM is ready AND window.L exists.
function luciReady() {
  return new Promise((resolve) => {
    if (window.L) {
      resolve(window.L)
      return
    }
    document.addEventListener('DOMContentLoaded', function wait() {
      if (window.L) {
        resolve(window.L)
      }
      else {
        // LuCI sets window.L shortly after DOMContentLoaded; poll briefly.
        let n = 0
        const timer = setInterval(() => {
          n++
          if (window.L || n > 100) {
            clearInterval(timer)
            resolve(window.L)
          }
        }, 50)
      }
    })
  })
}

/**
 * Load LuCI's native clients. Cached; safe to call multiple times.
 * @returns {Promise<{rpc: Object, uci: Object, fs: Object}>}
 */
export function luciClients() {
  if (cached)
    return cached

  cached = luciReady().then((L) => Promise.all([
    L.require('rpc'),
    L.require('uci'),
    L.require('fs'),
    L.require('ui')
  ]).then(([rpc, uci, fs, ui]) => ({ rpc, uci, fs, ui })))

  return cached
}

/**
 * Call a rpcd method via LuCI's rpc module.
 * @param {string} object e.g. 'luci.ddns-rs'
 * @param {string} method
 * @param {object} [params]
 * @returns {Promise<any>}
 */
export async function call(object, method, params) {
  const { rpc } = await luciClients()
  const fn = rpc.declare({
    object,
    method,
    expect: {}
  })
  // rpc.declare returns a function; for methods with named params use
  // positional args if given, otherwise an object.
  return params ? fn(params) : fn()
}

/**
 * Create a typed call wrapper for a method.
 */
export function createApi(object, method) {
  return (params) => call(object, method, params)
}

/** Declare a set of methods for an object. */
export function declareApi(object, methods) {
  const api = {}
  for (const name of methods)
    api[name] = createApi(object, name)
  return api
}

/**
 * uci - read/write UCI config via LuCI's uci module.
 * Returns a synchronous wrapper; each method resolves the client lazily.
 */
export function uci(config) {
  return {
    async load() {
      const { uci: u } = await luciClients()
      await u.load(config)
      return u
    },
    async get(section, opt) {
      const u = await this.load()
      return u.get(config, section, opt)
    },
    async set(section, values) {
      const u = await this.load()
      await u.set(config, section, values)
    },
    async save() {
      const u = await this.load()
      await u.save()
    },
    async apply(timeout) {
      const u = await this.load()
      await u.apply(timeout || 10)
    }
  }
}

/**
 * file - run commands / read files via LuCI's fs module.
 * Returns a synchronous wrapper; each method resolves the client lazily.
 */
export function file() {
  return {
    async exec(command, params) {
      const { fs } = await luciClients()
      return fs.exec(command, params || [], {})
    },
    async read(path) {
      const { fs } = await luciClients()
      return fs.read(path)
    }
  }
}
