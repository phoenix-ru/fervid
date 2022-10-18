<abc-def
  v-model.lazy="modelValue"
  v-test-directive:test-argument.foo.bar="foo-bar"
  :test-bound="bar-baz"
  #custom-slot="{ prop }"
  disabled
  class=""
  @click.prevent
  @hello="world"
>