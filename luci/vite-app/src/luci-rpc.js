// luci-rpc.js - minimal ubus JSON-RPC client for ddns-rs LuCI.
// Mirrors LuCI's rpc protocol: POST {jsonrpc, id, method: 'call',
// params: [sessionid, object, method, params]} to /admin/ubus.
// Session id is read from LuCI's global L.env (available because the SPA
// is embedded in a LuCI page). If L is absent (e.g. standalone dev), the
// caller can set sessionId explicitly.

let rpcSessionID = null
let rpcRequestID = 1
const rpcBaseURL = '/cgi-bin/luci/admin/ubus'

function getSessionID() {
  if (rpcSessionID != null)
    return rpcSessionID
  if (window.L && window.L.env && window.L.env.sessionid)
    return window.L.env.sessionid
  return '00000000000000000000000000000000'
}

export function setSessionID(sid) {
  rpcSessionID = sid
}

/**
 * Invoke a rpcd method on the given object.
 * @param {string} object  e.g. 'luci.ddns-rs'
 * @param {string} method  e.g. 'binary_status'
 * @param {object} params  positional or object params
 * @returns {Promise<any>} the ubus result payload
 */
export async function call(object, method, params) {
  const msg = {
    jsonrpc: '2.0',
    id: rpcRequestID++,
    method: 'call',
    params: [getSessionID(), object, method, params || {}]
  }

  const resp = await fetch(rpcBaseURL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'same-origin',
    body: JSON.stringify(msg)
  })

  if (!resp.ok)
    throw new Error(`RPC HTTP ${resp.status}`)

  const data = await resp.json()

  if (data.error && data.error.code && data.error.message)
    throw new Error(`RPC ${data.error.message}`)

  if (Array.isArray(data.result)) {
    const [code, payload] = data.result
    if (code !== 0)
      throw new Error(`ubus error ${code}`)
    return payload
  }

  return data.result
}

/**
 * Create a typed call wrapper for a method.
 * @param {string} object
 * @param {string} method
 * @returns {function(params?: object): Promise<any>}
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
 * uci - read/write UCI config via ubus (mirrors LuCI's uci module).
 * @param {string} config e.g. 'ddns-rs'
 * @returns {{ load: function(): Promise<object>, set: function(section, values): Promise<*> }}
 */
export function uci(config) {
  const api = declareApi('uci', ['get', 'set', 'save', 'apply', 'commit'])

  return {
    /** Load the whole config; resolves to the values object. */
    async load() {
      const res = await api.get(config)
      return (res && res.values) || {}
    },
    /** Set option values on a section and commit. */
    async set(section, values) {
      await api.set(config, section, values)
      await api.save(config)
    }
  }
}

/**
 * file - run commands / read files via ubus (mirrors LuCI's fs module).
 * @returns {{ exec: function(cmd, args?): Promise<{code,stdout,stderr}>, read: function(path): Promise<string> }}
 */
export function file() {
  const api = declareApi('file', ['exec', 'read', 'write'])

  return {
    async exec(command, args) {
      return api.exec(command, args || [], {})
    },
    async read(path) {
      const res = await api.read(path)
      return (res && res.data) || ''
    }
  }
}
