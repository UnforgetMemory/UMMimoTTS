<template>
  <BatchImportWizard
    :open="open"
    @update:open="$emit('update:open', $event)"
    @imported="$emit('imported', $event)"
  />
</template>

<script setup lang="ts">
import BatchImportWizard from './BatchImportWizard.vue'

interface Props {
  open: boolean
}

interface Emits {
  (e: 'update:open', value: boolean): void
  (e: 'imported', groupId: string): void
}

defineProps<Props>()
defineEmits<Emits>()
</script>
