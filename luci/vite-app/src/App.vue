<template>
  <template v-if="page === 'binary'">
    <BinaryStatusCard />
    <BinaryActions />
  </template>
  <BaseSetting v-else-if="page === 'config'" />
  <LogView v-else-if="page === 'log'" />
</template>

<script setup>
import { defineProps } from 'vue'
import BinaryStatusCard from './components/BinaryStatusCard.vue'
import BinaryActions from './components/BinaryActions.vue'
import BaseSetting from './components/BaseSetting.vue'
import LogView from './components/LogView.vue'

defineProps({ page: { type: String, default: 'binary' } })
</script>
