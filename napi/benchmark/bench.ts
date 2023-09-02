import b from 'benny'

import { compileSync } from '../index'
import { compileTemplate } from '@vue/compiler-sfc'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { cwd } from 'node:process'

const input = readFileSync(join(cwd(), '../crates/fervid/benches/fixtures/input.vue'), {
  encoding: 'utf-8',
})

async function run() {
  await b.suite(
    'compile sfc',

    b.add('@vue/compiler-sfc', () => {
      compileTemplate({
        filename: 'input.vue',
        source: input,
        id: '',
      })
    }),

    b.add('@fervid/napi', () => {
      compileSync(input)
    }),

    b.cycle(),
    b.complete(),
  )
}

run().catch((e) => {
  console.error(e)
})
