import { describe, expect, test } from 'vitest'
import { assertCode, compile } from './utils'

describe('defineSlots()', () => {
  test('basic usage', () => {
    const { content } = compile(`
      <script setup lang="ts">
      const slots = defineSlots<{
        default: { msg: string }
      }>()
      </script>
    `)
    assertCode(content)
    expect(content).toMatch(`const slots = _useSlots()`)
    expect(content).not.toMatch('defineSlots')
  })

  test('w/o return value', () => {
    const { content } = compile(`
      <script setup lang="ts">
      defineSlots<{
        default: { msg: string }
      }>()
      </script>
    `)
    assertCode(content)
    expect(content).not.toMatch('defineSlots')
    expect(content).not.toMatch(`_useSlots`)
  })

  test('w/o generic params', () => {
    const { content } = compile(`
      <script setup>
      const slots = defineSlots()
      </script>
    `)
    assertCode(content)
    expect(content).toMatch(`const slots = _useSlots()`)
    expect(content).not.toMatch('defineSlots')
  })

  test('error on duplicate', () => {
    const { errors } = compile(`
      <script setup>
      defineSlots()
      defineSlots()
      </script>
    `)
    expect(errors.length).toBe(1)
    expect(errors[0].message).toMatch('DuplicateDefineSlots')
  })

  test('error on arguments', () => {
    const { errors } = compile(`
      <script setup>
      const slots = defineSlots({
        default: () => {}
      })
      </script>
    `)
    expect(errors.length).toBe(1)
    expect(errors[0].message).toMatch('DefineSlotsArguments')
  })
})
