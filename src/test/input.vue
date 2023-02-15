<template>
  <abc-def
    v-model.lazy="modelValue"
    v-model:another-model-value.trim="modelValue"
    v-test-directive:test-argument.foo.bar="foo - bar"
    :test-bound="bar + baz"
    disabled
    class=""
    @click.prevent
    @hello="world"
  >
    The text of the node

    {{ dynamicValue }}

    <!-- Comment -->
    <another-element />

    yet another text

    <template v-slot:test-slot="{ value }">
      test {{ value }}
    </template>

    <template #custom-slot="{ prop }">
      <span class="span-class">
        Span text
      </span>
      {{ prop }}
    </template>

    <input v-model="inputModel" v-directive:foo.bar.buzz="baz">
  </abc-def>
</template>

<script>
import { defineComponent, ref } from 'vue'

export default defineComponent({
  setup() {
    return {
      inputModel: ref(''),
      modelValue: ref('')
    }
  },
})
</script>

<style scoped>
.span-class {
  background: yellow;
}
</style>
