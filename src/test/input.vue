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

    <!-- Todo remove space between these elements, otherwise it breaks the invariant in conditional codegen -->
    <div v-if="true">if div</div>
    <span v-else-if="false">else-if span</span>

    <span v-for="i in list" :key="i">hey</span>
    <br v-show="false">

    <template v-for="i in list">
      hey
      <span :key="i">{{ i }}</span>
    </template>
  </abc-def>
</template>

<script>
import { defineComponent, ref } from 'vue'

export default defineComponent({
  setup() {
    return {
      inputModel: ref(''),
      modelValue: ref(''),
      list: [1, 2, 3]
    }
  },
})
</script>

<style scoped>
.span-class {
  background: yellow;
}
</style>
