<template>
  <div class="cbi-section">
    <h3>{{ t('DDNS-RS Binary Status') }}</h3>
    <div v-if="error" class="cbi-section-descr" style="color:red">
      {{ t('Status check error') }}
    </div>
    <div v-else class="cbi-map">
      <div class="tr">
        <div class="td left" style="width:240px">{{ t('Installed') }}</div>
        <div class="td left">{{ binary.installed ? t('Yes') : t('No') }}</div>
      </div>
      <div class="tr">
        <div class="td left" style="width:240px">{{ t('Version') }}</div>
        <div class="td left">{{ binary.version || '-' }}</div>
      </div>
      <div class="tr">
        <div class="td left" style="width:240px">{{ t('Binary path') }}</div>
        <div class="td left">{{ binary.path || '-' }}</div>
      </div>
      <div class="tr">
        <div class="td left" style="width:240px">{{ t('Service') }}</div>
        <div class="td left">{{ service.running ? t('Running') : t('Stopped') }}</div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, reactive, onMounted, onUnmounted } from 'vue'
import { declareApi } from '../luci-rpc'
import { useI18n } from '../i18n'

const { t } = useI18n()

const api = declareApi('luci.ddns-rs', ['status'])

const binary = reactive({ installed: false, version: '', path: '' })
const service = reactive({ running: false })
const error = ref(false)

let timer = null

async function refresh() {
  try {
    const st = await api.status()
    binary.installed = !!(st && st.binary && st.binary.installed)
    binary.version = (st && st.binary && st.binary.version) || ''
    binary.path = (st && st.binary && st.binary.path) || ''
    service.running = !!(st && st.service && st.service.running)
    error.value = false
  }
  catch (e) {
    error.value = true
  }
}

onMounted(() => {
  refresh()
  timer = setInterval(refresh, 5000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>
