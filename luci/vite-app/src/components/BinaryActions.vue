<template>
  <div class="cbi-section">
    <h3>{{ t('Install Binary') }}</h3>

    <!-- upload -->
    <div class="cbi-value">
      <button class="btn cbi-button-action" :disabled="busy" @click="pickFile">
        {{ t('Upload & Install') }}
      </button>
    </div>

    <!-- install from URL -->
    <div class="cbi-value">
      <input
        v-model="url"
        type="text"
        class="cbi-input-text"
        :placeholder="t('Enter a direct download URL of the ddns-rs binary or .tar.gz archive.')"
        style="width:100%;margin-bottom:5px"
      />
      <button class="btn cbi-button-action" :disabled="busy" @click="installFromUrl">
        {{ t('Install from URL') }}
      </button>
    </div>

    <!-- auto update -->
    <h3>{{ t('Auto Install / Update') }}</h3>
    <div class="cbi-value">
      <button class="btn cbi-button-action" :disabled="busy" @click="autoUpdate">
        {{ busy ? t('Working...') : t('Auto Install/Update') }}
      </button>
    </div>

    <!-- progress log -->
    <pre v-if="busy && log" class="ddns-rs-log"
      style="max-height:12em;overflow:auto;background:#111;color:#eee;padding:0.5em">{{ log }}</pre>

    <!-- result / error -->
    <p v-if="result" class="cbi-section-descr">{{ result }}</p>
    <p v-if="errorMsg" class="cbi-section-descr" style="color:red">{{ errorMsg }}</p>
  </div>
</template>

<script setup>
import { ref, reactive, onUnmounted } from 'vue'
import { declareApi, luciClients } from '../luci-rpc'
import { useI18n } from '../i18n'

const { t } = useI18n()

const api = declareApi('luci.ddns-rs', [
  'binary_install',
  'binary_upload',
  'binary_update',
  'binary_progress'
])

const url = ref('')
const busy = ref(false)
const log = ref('')
const result = ref('')
const errorMsg = ref('')

let pollTimer = null
let uploadPath = ''

function setBusy(on) {
  busy.value = on
  if (on) {
    log.value = ''
    result.value = ''
    errorMsg.value = ''
    pollTimer = setInterval(pollProgress, 1000)
  }
  else if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

async function pollProgress() {
  try {
    const p = await api.binary_progress()
    if (p && typeof p.text === 'string')
      log.value = p.text
  }
  catch (e) { /* ignore */ }
}

function showOutcome(reply) {
  setBusy(false)
  if (reply && reply.ok === false)
    errorMsg.value = reply.message || t('Operation failed')
  else
    result.value = t('Operation completed.')
}

function runAction(fn) {
  setBusy(true)
  fn()
    .then(showOutcome)
    .catch(err => {
      setBusy(false)
      errorMsg.value = String(err)
    })
}

async function pickFile() {
  const { ui } = await luciClients()
  uploadPath = `/tmp/ddns-rs-upload-${Date.now()}`

  setBusy(true)
  try {
    await ui.uploadFile(uploadPath)
    const reply = await api.binary_upload({ path: uploadPath })
    showOutcome(reply)
  }
  catch (err) {
    setBusy(false)
    if (err && err.message && err.message.indexOf('cancelled') >= 0)
      return
    errorMsg.value = String(err)
  }
}

function installFromUrl() {
  if (!url.value.trim()) {
    errorMsg.value = t('Please enter a download URL')
    return
  }
  runAction(() => api.binary_install({ url: url.value.trim() }))
}

function autoUpdate() {
  runAction(() => api.binary_update())
}

onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer)
})
</script>
