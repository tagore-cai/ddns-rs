<template>
  <div class="cbi-map">
    <div class="cbi-section" id="status_bar">
      <em>
        <span :style="{ color: running ? 'green' : 'red' }">
          {{ running ? '✓' : '✗' }}
          <strong>DDNS-RS {{ version ? 'v' + version : '' }} - {{ running ? t('RUNNING') : t('NOT RUNNING') }}</strong>
        </span>
        <a v-if="running" class="btn cbi-button" :href="webUrl" target="_blank" style="margin-left:0.5em">
          {{ t('Open Web Interface') }}
        </a>
      </em>
      <p class="cbi-section-descr">
        {{ t('Default web interface login: Username: admin, Password: admin12345') }}
      </p>
    </div>

    <div class="cbi-section">
      <h3>{{ t('Base Setting') }}</h3>

      <label class="cbi-value">
        <input type="checkbox" v-model="form.enabled" :checked="form.enabled === '1'" @change="onEnabledChange" />
        {{ t('Enable') }}
      </label>

      <label class="cbi-value">
        <span style="width:240px;display:inline-block">{{ t('Listen address') }}</span>
        <input type="text" class="cbi-input-text" v-model="form.port" />
        <small class="cbi-section-descr">{{ t('Full listen address, e.g. [::]:9876 or 0.0.0.0:9876') }}</small>
      </label>

      <label class="cbi-value">
        <span style="width:240px;display:inline-block">{{ t('Update interval (seconds)') }}</span>
        <input type="number" class="cbi-input-text" v-model.number="form.time" />
        <small class="cbi-section-descr">{{ t('Update interval in seconds (60-86400)') }}</small>
      </label>

      <label class="cbi-value">
        <span style="width:240px;display:inline-block">{{ t('Provider comparison interval') }}</span>
        <input type="number" class="cbi-input-text" v-model.number="form.ctimes" />
        <small class="cbi-section-descr">{{ t('Number of times to compare with service provider (1-60)') }}</small>
      </label>

      <label class="cbi-value">
        <span style="width:240px;display:inline-block">{{ t('Skip verifying certificates') }}</span>
        <select class="cbi-input-select" v-model="form.skipverify">
          <option value="0">{{ t('No') }}</option>
          <option value="1">{{ t('Yes') }}</option>
        </select>
      </label>

      <label class="cbi-value">
        <span style="width:240px;display:inline-block">{{ t('Specify DNS resolution server') }}</span>
        <select class="cbi-input-select" v-model="form.dns">
          <option value="223.5.5.5">Ali DNS 223.5.5.5</option>
          <option value="223.6.6.6">Ali DNS 223.6.6.6</option>
          <option value="119.29.29.29">Tencent DNS 119.29.29.29</option>
          <option value="1.1.1.1">CloudFlare DNS 1.1.1.1</option>
          <option value="8.8.8.8">Google DNS 8.8.8.8</option>
          <option value="8.8.4.4">Google DNS 8.8.4.4</option>
        </select>
      </label>

      <label class="cbi-value">
        <input type="checkbox" v-model="form.noweb" :checked="form.noweb === '1'" @change="onNowebChange" />
        {{ t('Do not start web services') }}
      </label>

      <label class="cbi-value">
        <span style="width:240px;display:inline-block">{{ t('Delayed Start (seconds)') }}</span>
        <input type="number" class="cbi-input-text" v-model.number="form.delay" />
      </label>

      <div class="cbi-page-actions" style="margin-top:1em">
        <input type="button" class="btn cbi-button-apply" :value="t('Save & Apply')" @click="save" :disabled="saving" />
        <input type="button" class="btn cbi-button-reset" :value="t('Reset')" @click="load" />
        <input type="button" class="btn cbi-button-action" :value="t('Reset account password')" @click="resetPassword" />
        <span v-if="saved" style="margin-left:1em;color:green">{{ t('Saved. Restart the service to apply.') }}</span>
        <span v-if="saveError" style="margin-left:1em;color:red">{{ saveError }}</span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, reactive, onMounted, onUnmounted } from 'vue'
import { declareApi, uci, file } from '../luci-rpc'
import { useI18n } from '../i18n'

const { t } = useI18n()

const ddnsApi = declareApi('luci.ddns-rs', ['status', 'get_ver'])
const fs = file()
const uciConf = uci('ddns-rs')

const form = reactive({
  enabled: '0', port: '[::]:9876', time: 300, ctimes: 5,
  skipverify: '0', dns: '223.5.5.5', noweb: '0', delay: 0
})

const running = ref(false)
const version = ref('')
const saving = ref(false)
const saved = ref(false)
const saveError = ref('')

let timer = null

function webUrl() {
  const port = String(form.port || '9876').split(':').pop()
  return `http://${window.location.hostname}:${port}`
}

async function refreshStatus() {
  try {
    const st = await ddnsApi.status()
    running.value = !!(st && st.service && st.service.running)
    const ver = await ddnsApi.get_ver()
    version.value = (ver && ver.ver && ver.ver.version) || ''
  }
  catch (e) { /* ignore */ }
}

async function load() {
  try {
    const values = await uciConf.load()
    const cfg = (values && values.config) || {}
    form.enabled = cfg.enabled || '0'
    form.port = cfg.port || '[::]:9876'
    form.time = parseInt(cfg.time || '300', 10)
    form.ctimes = parseInt(cfg.ctimes || '5', 10)
    form.skipverify = cfg.skipverify || '0'
    form.dns = cfg.dns || '223.5.5.5'
    form.noweb = cfg.noweb || '0'
    form.delay = parseInt(cfg.delay || '0', 10)
  }
  catch (e) {
    saveError.value = String(e)
  }
}

function onEnabledChange(e) {
  form.enabled = e.target.checked ? '1' : '0'
}
function onNowebChange(e) {
  form.noweb = e.target.checked ? '1' : '0'
}

async function save() {
  saving.value = true
  saved.value = false
  saveError.value = ''
  try {
    const values = {
      enabled: form.enabled,
      port: form.port,
      time: String(form.time),
      ctimes: String(form.ctimes),
      skipverify: form.skipverify,
      dns: form.dns,
      noweb: form.noweb,
      delay: String(form.delay)
    }
    await uciConf.set('config', values)
    // restart the service to apply
    await fs.exec('/etc/init.d/ddns-rs', ['restart'])
    saved.value = true
    await refreshStatus()
  }
  catch (e) {
    saveError.value = String(e)
  }
  finally {
    saving.value = false
  }
}

async function resetPassword() {
  if (!window.confirm(t('Reset web interface password to admin/admin12345?')))
    return
  try {
    await fs.exec('/usr/bin/ddns-rs', ['-resetPassword', 'admin12345', '-c', '/etc/ddns-rs/ddns-rs-config.yaml'])
    await fs.exec('/etc/init.d/ddns-rs', ['restart'])
    window.alert(t('Password reset. Restart the service to apply.'))
  }
  catch (e) {
    window.alert(String(e))
  }
}

onMounted(() => {
  load()
  refreshStatus()
  timer = setInterval(refreshStatus, 5000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>
