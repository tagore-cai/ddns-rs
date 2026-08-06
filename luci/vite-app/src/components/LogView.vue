<template>
  <div class="cbi-map">
    <style>
      .ddns-rs-log-container { max-height: 1200px; overflow-y: auto; border-radius: 3px; margin-top: 10px; padding: 5px; background-color: var(--background-color); font-family: monospace; font-size: 12px; border: 1px solid var(--border-color); min-height: 200px; }
      .ddns-rs-log-line { padding: 3px 5px; font-family: monospace; font-size: 12px; line-height: 1.4; border-bottom: 1px solid var(--border-color-light); white-space: pre-wrap; word-break: break-all; }
      .ddns-rs-log-line:last-child { border-bottom: none; }
      .ddns-rs-log-timestamp { color: #0066cc; margin-right: 10px; font-weight: bold; }
      .ddns-rs-log-error { color: #cc0000; }
      .ddns-rs-log-warning { color: #ff9900; }
    </style>
    <div class="cbi-section">
      <div style="margin-bottom:10px;display:flex;gap:5px">
        <button class="cbi-button cbi-button-remove" @click="clearLogs" :disabled="clearing">
          {{ clearing ? t('Clearing...') : t('Clear Logs') }}
        </button>
        <button class="cbi-button cbi-button-action" @click="fetchLogs">{{ t('Refresh') }}</button>
      </div>
      <div ref="logContainer" class="ddns-rs-log-container">
        <div v-if="loading" class="ddns-rs-log-line">{{ t('Loading logs...') }}</div>
        <div v-else-if="error" class="ddns-rs-log-line ddns-rs-log-error">{{ error }}</div>
        <div v-else-if="lines.length === 0" class="ddns-rs-log-line">{{ t('No ddns-rs logs found.') }}</div>
        <div v-else v-for="(ln, i) in lines" :key="i" :class="lineClass(ln)">
          <span v-if="ln.timestamp" class="ddns-rs-log-timestamp">{{ ln.timestamp }} </span>
          <span>{{ ln.message }}</span>
        </div>
      </div>
      <small>{{ t('Refresh every 5 seconds.') }}</small>
    </div>
  </div>
</template>

<script setup>
import { ref, reactive, onMounted, onUnmounted, nextTick } from 'vue'
import { file } from '../luci-rpc'
import { useI18n } from '../i18n'

const { t } = useI18n()
const fs = file()

const logContainer = ref(null)
const lines = ref([])
const loading = ref(true)
const clearing = ref(false)
const error = ref('')

let timer = null

function extract(line) {
  if (!line || !line.includes('ddns-rs'))
    return null
  const ts1 = line.match(/^([A-Z][a-z]{2}\s+[A-Z][a-z]{2}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}\s+\d{4})/)
  if (ts1)
    return { timestamp: ts1[1], message: line.slice(ts1[0].length).replace(/^:\s*/, '') }
  const ts2 = line.match(/(\d{4}\/\d{2}\/\d{2}\s+\d{2}:\d{2}:\d{2})\s+(.*)$/)
  if (ts2)
    return { timestamp: ts2[1], message: ts2[2] }
  return { timestamp: null, message: line }
}

function lineClass(ln) {
  const cls = ['ddns-rs-log-line']
  const m = ln.message
  if (m.includes('err') || m.includes('ERROR') || m.includes('failed'))
    cls.push('ddns-rs-log-error')
  else if (m.includes('warn') || m.includes('WARNING'))
    cls.push('ddns-rs-log-warning')
  return cls
}

async function fetchLogs() {
  try {
    const res = await fs.exec('/usr/libexec/ddns-rs-call', ['get_logs'])
    const text = (res && res.stdout) || ''
    const parsed = text.split('\n')
      .map(l => l.trim())
      .filter(l => l && !l.includes('No ddns-rs logs found'))
      .map(extract)
      .filter(l => l)
    parsed.reverse()
    lines.value = parsed
    error.value = ''
  }
  catch (e) {
    error.value = t('Failed to read logs: %s').replace('%s', String(e.message || e))
  }
  finally {
    loading.value = false
    nextTick(() => {
      if (logContainer.value)
        logContainer.value.scrollTop = 0
    })
  }
}

async function clearLogs() {
  clearing.value = true
  try {
    await fs.exec('/usr/libexec/ddns-rs-call', ['clear_logs'])
    lines.value = []
    await fetchLogs()
  }
  catch (e) {
    error.value = String(e)
  }
  finally {
    clearing.value = false
  }
}

onMounted(() => {
  fetchLogs()
  timer = setInterval(fetchLogs, 5000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>
