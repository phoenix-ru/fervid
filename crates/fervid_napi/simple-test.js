const fs = require('node:fs')
const path = require('node:path')
const { compileSync } = require('./index')

const input = fs.readFileSync(path.join(process.cwd(), '../crates/fervid/benches/fixtures/input.vue'), {
  encoding: 'utf-8',
})

const compiledCode = compileSync(input)

console.log(compiledCode)
