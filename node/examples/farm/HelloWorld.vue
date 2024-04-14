<template>
    <h1>
        Hello {{ name }}!
    </h1>
    <input v-model="name">
    <HelloInput v-model="name"></HelloInput>
</template>

<script setup>
import { ref } from 'vue'
import HelloInput from './HelloInput.vue'

const name = ref('fervid')
</script>
