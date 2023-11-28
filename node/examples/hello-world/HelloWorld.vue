<template>
    <h1>
        Hello {{ name }}!
    </h1>
    <input v-model="name">

    <!-- This is a challenge: SWC does not accept `HelloInput` and modifies the tag name to `helloinput` -->
    <hello-input v-model="name"></hello-input>
</template>

<script setup>
import { ref } from 'vue'
import HelloInput from './HelloInput.vue'

const name = ref('fervid')
</script>

<script>
// This hack is needed because `fervid` doesn't support component resolution yet
// TODO Remove when component resolutions are properly supported
export default {
    components: { HelloInput }
}
</script>
