import { describe, expect, it, test } from 'vitest'
import { assertCode, compile } from './utils'

describe('defineOptions()', () => {
    test('basic usage', () => {
        const { content } = compile(`
            <script setup>
            defineOptions({ name: 'FooApp' })
            </script>
        `)
        assertCode(content)
        // should remove defineOptions import and call
        expect(content).not.toMatch('defineOptions')
        // should include context options in default export
        expect(content).toMatch(
            `export default {
    name: 'FooApp',`,
        )
    })

    test('empty argument', () => {
        const { content } = compile(`
            <script setup>
            defineOptions()
            </script>
        `)
        assertCode(content)
        expect(content).toMatch(`export default {`)
        // should remove defineOptions import and call
        expect(content).not.toMatch('defineOptions')
    })

    it('should emit an error with two defineOptions', () => {
        const { errors } = compile(`
            <script setup>
            defineOptions({ name: 'FooApp' })
            defineOptions({ name: 'BarApp' })
            </script>
        `)
        expect(errors.length).toBe(1)
        expect(errors[0].message).toMatch('DuplicateDefineOptions')
    })

    it('should emit an error with props or emits property', () => {
        {
            const { errors } = compile(`
                <script setup>
                defineOptions({ props: { foo: String } })
                </script>
            `)
            expect(errors.length).toBe(1)
            expect(errors[0].message).toMatch('DefineOptionsProps')
        }

        {
            const { errors } = compile(`
                <script setup>
                defineOptions({ emits: ['update'] })
                </script>
            `)
            expect(errors.length).toBe(1)
            expect(errors[0].message).toMatch('DefineOptionsEmits')
        }

        {
            const { errors } = compile(`
                <script setup>
                defineOptions({ expose: ['foo'] })
                </script>
            `)
            expect(errors.length).toBe(1)
            expect(errors[0].message).toMatch('DefineOptionsExpose')
        }

        {
            const { errors } = compile(`
                <script setup>
                defineOptions({ slots: ['foo'] })
                </script>
            `)
            expect(errors.length).toBe(1)
            expect(errors[0].message).toMatch('DefineOptionsSlots')
        }
    })

    it('should emit an error with type generic', () => {
        const { errors } = compile(`
            <script setup lang="ts">
            defineOptions<{ name: 'FooApp' }>()
            </script>
        `)
        expect(errors.length).toBe(1)
        expect(errors[0].message).toMatch('DefineOptionsTypeArguments')
    })

    it('should emit an error with type assertion', () => {
        const { errors } = compile(`
            <script setup lang="ts">
            defineOptions({ props: [] } as any)
            </script>
        `)
        expect(errors.length).toBe(1)
        expect(errors[0].message).toMatch('DefineOptionsProps')
    })

    it('should emit an error with declaring props/emits/slots/expose', () => {
        {
            const { errors } = compile(`
                  <script setup>
                  defineOptions({ props: ['foo'] })
                  </script>
            `)
            expect(errors.length).toBe(1)
            expect(errors[0].message).toMatch('DefineOptionsProps')
        }

        {
            const { errors } = compile(`
                  <script setup>
                  defineOptions({ emits: ['update'] })
                  </script>
            `)
            expect(errors.length).toBe(1)
            expect(errors[0].message).toMatch('DefineOptionsEmits')
        }

        {
            const { errors } = compile(`
                  <script setup>
                  defineOptions({ expose: ['foo'] })
                  </script>
            `)
            expect(errors.length).toBe(1)
            expect(errors[0].message).toMatch('DefineOptionsExpose')
        }

        {
            const { errors } = compile(`
                  <script setup lang="ts">
                  defineOptions({ slots: Object })
                  </script>
            `)
            expect(errors.length).toBe(1)
            expect(errors[0].message).toMatch('DefineOptionsSlots')
        }
    })
})
