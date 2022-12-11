<template>
  <abc-def
    v-model.lazy="modelValue"
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
      test
    </template>

    <template #custom-slot="{ prop }">
      <span class="span-class">
        Span text
      </span>
    </template>
  </abc-def>
</template>

<script>
import { defineComponent } from 'vue'

export default defineComponent({
  setup() {
    
  },
})
</script>
