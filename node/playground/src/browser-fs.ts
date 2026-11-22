// Monaco's TypeScript service references fs only for optional Node-only trace files
// Browser language services do not use it; fail explicitly if file tracing is requested
export function existsSync(): never {
    throw new Error('TypeScript file tracing requires Node.js fs')
}
