import { createApp } from 'vue'
import App from './App.vue'

// The LuCI views (binary.js / config.js / log.js) load this bundle and call
// window.__DDNS_RS_APP__.mount(container, page) once ready.
// page is 'binary' | 'config' | 'log' (defaults to auto-detect from URL).
window.__DDNS_RS_APP__ = {
  mount: function (container, page) {
    if (!container)
      container = document.getElementById('ddns-rs-app')
    if (!container)
      return

    if (!page) {
      const p = window.location.pathname.split('/').pop() || ''
      if (p.indexOf('config') !== -1) page = 'config'
      else if (p.indexOf('log') !== -1) page = 'log'
      else page = 'binary'
    }

    const app = createApp(App, { page })
    app.mount(container)
  }
}
