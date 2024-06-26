import b from 'benny'
import format from 'benny/lib/internal/format'
import type { CaseResultWithDiff, Summary } from 'benny/lib/internal/common-types'
import kleur from 'kleur'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { cpus } from 'node:os'
import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc'

import { Compiler, FervidCompileOptions } from '../index'

// Increase libuv thread pool for a better async result.
// 4 threads is a default thread pool size.
const CPUS = cpus().length - 1
process.env.UV_THREADPOOL_SIZE = CPUS.toString()

const input = readFileSync(join(__dirname, '../../fervid/benches/fixtures/input.vue'), {
  encoding: 'utf-8',
})

const options: FervidCompileOptions = {
  filename: 'input.vue',
  id: ''
}

async function run() {
  const compiler = new Compiler()

  await b.suite(
    'compile sfc',

    b.add('@vue/compiler-sfc', () => {
      const descriptor = parse(input, { filename: 'input.vue' })
      if (descriptor.descriptor.script || descriptor.descriptor.scriptSetup) {
        compileScript(descriptor.descriptor, {
          id: 'abc',
          isProd: true,
          inlineTemplate: true,
        })
      } else {
        compileTemplate({
          source: descriptor.descriptor.source,
          filename: 'input.vue',
          id: 'abc',
        })
      }
    }),

    b.add('@fervid/napi sync', () => {
      compiler.compileSync(input, options)
    }),

    // The code below makes sure that async framework is not flawed.
    // On my PC `sync promise` benches produce results close to a `sync` bench,
    // which is expected, because `compileSync` is blocking.
    // The `async` benches are properly multithreaded, thus they achieve much higher ops/sec.
    // BEGIN

    b.add('@fervid/napi sync promise (4 threads)', () => {
      return Promise.allSettled(
        Array.from({ length: 4 }, (_) => new Promise<void>((resolve) => (compiler.compileSync(input, options), resolve()))),
      )
    }),

    b.add(`@fervid/napi sync promise (${CPUS} threads)`, () => {
      return Promise.allSettled(
        Array.from({ length: CPUS }, (_) => new Promise<void>((resolve) => (compiler.compileSync(input, options), resolve()))),
      )
    }),

    // END

    b.add('@fervid/napi async (4 threads)', () => {
      return Promise.allSettled(Array.from({ length: 4 }, (_) => compiler.compileAsync(input, options)))
    }),

    b.add(`@fervid/napi async CPUS (${CPUS} threads)`, () => {
      return Promise.allSettled(Array.from({ length: CPUS }, (_) => compiler.compileAsync(input, options)))
    }),

    // Custom cycle function to account for the async nature
    // Copied from `benny` and adjusted
    b.cycle((_, summary) => {
      const allCompleted = summary.results.every((item) => item.samples > 0)
      const fastestOps = format(summary.results[summary.fastest.index].ops)
      const progress = Math.round(
        (summary.results.filter((result) => result.samples !== 0).length / summary.results.length) * 100,
      )

      const progressInfo = `Progress: ${progress}%`

      // Compensate for async
      if (progress === 100) {
        for (const result of summary.results) {
          const match = result.name.match(/\((\d+) threads\)/)
          if (!match || !match[1] || isNaN(+match[1])) continue

          result.ops *= +match[1]
        }
      }

      // Re-map fastest/slowest
      const fastest = summary.results.reduce(
        (prev, next, index) => {
          return next.ops > prev.ops ? { ops: next.ops, index, name: next.name } : prev
        },
        { ops: 0, index: 0, name: '' },
      )
      const slowest = summary.results.reduce(
        (prev, next, index) => {
          return next.ops < prev.ops ? { ops: next.ops, index, name: next.name } : prev
        },
        { ops: Infinity, index: 0, name: '' },
      )
      summary.fastest = fastest
      summary.slowest = slowest
      summary.results.forEach((result, index) => {
        result.percentSlower = index === fastest.index ? 0 : Number(((1 - result.ops / fastest.ops) * 100).toFixed(2))
      })

      const output = summary.results
        .map((item, index) => {
          const ops = format(item.ops)
          const margin = item.margin.toFixed(2)

          return item.samples
            ? kleur.cyan(`\n  ${item.name}:\n`) +
                `    ${ops} ops/s, ±${margin}% ${allCompleted ? getStatus(item, index, summary, ops, fastestOps) : ''}`
            : null
        })
        .filter((item) => item != null)
        .join('\n')

      return `${progressInfo}\n${output}`
    }),

    b.complete(),
  )
}

run().catch((e) => {
  console.error(e)
})

function getStatus(item: CaseResultWithDiff, index: number, summary: Summary, ops: string, fastestOps: string) {
  const isFastest = index === summary.fastest.index
  const isSlowest = index === summary.slowest.index
  const statusShift = fastestOps.length - ops.length + 2
  return (
    ' '.repeat(statusShift) +
    (isFastest
      ? kleur.green('| fastest')
      : isSlowest
        ? kleur.red(`| slowest, ${item.percentSlower}% slower`)
        : kleur.yellow(`| ${item.percentSlower}% slower`))
  )
}
