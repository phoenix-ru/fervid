<template>
    <h1>
        Hello {{ name }}!
    </h1>
    <input v-model="name">
</template>

<script setup>
import { ref } from 'vue'

const name = ref('fervid')
</script>
