import { test, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { compileSync } from '../index'

const input = readFileSync(join(__dirname, '../../fervid/benches/fixtures/input.vue'), {
  encoding: 'utf-8',
})

test('should work', () => {
  expect(compileSync(input)).toMatchSnapshot()
})
