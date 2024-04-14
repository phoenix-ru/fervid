<template>
  <div class="hello-input">
    <input
      v-model="innerModelValue"
      :class="{
        'w-full': fullwidth
      }"
      class="hello-input__input"
    >
  </div>
</template>

<script lang="ts" setup>
import { computed, ref, watch } from 'vue'

const props = defineProps({
  fullwidth: {
    type: Boolean,
    required: false,
    default: true
  },
  modelValue: {
    type: String,
    required: true
  }
})

const emit = defineEmits(['update:model-value'])

const innerModelValue = ref(props.modelValue)
const computedModelValue = computed(() => props.modelValue)
watch(computedModelValue, (value) => {
  innerModelValue.value = value
})

watch([innerModelValue], (values) => {
  emit('update:model-value', values[0])
})
</script>

<style scoped>
.hello-input__input {
  padding: 1rem 1.5rem;
  border: 1px solid rgba(0, 0, 0, 0.2);
  border-radius: 999px;
}

.hello-input__input:hover, .hello-input__input:focus {
  border-color: #000;
}
</style>
