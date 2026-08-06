import { createApp } from 'vue'
import App from './App.vue'

// The LuCI view (binary-vue.js) loads this bundle and calls
// window.__DDNS_RS_APP__.mount(container) once the script is ready.
window.__DDNS_RS_APP__ = {
  mount: function (container) {
    if (!container)
      container = document.getElementById('ddns-rs-app')
    if (!container)
      return
    createApp(App).mount(container)
  }
}
