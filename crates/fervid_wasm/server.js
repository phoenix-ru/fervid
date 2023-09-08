// Adapted from https://gist.github.com/aolde/8104861

const http = require('http')
const url = require('url')
const path = require('path')
const fs = require('fs/promises')

const port = process.argv[2] || 8888

const mimeTypes = {
    html: 'text/html',
    jpeg: 'image/jpeg',
    jpg: 'image/jpeg',
    png: 'image/png',
    svg: 'image/svg+xml',
    json: 'application/json',
    js: 'text/javascript',
    css: 'text/css',
    wasm: 'application/wasm'
}

// Headers required for high resolution timers: https://developer.mozilla.org/en-US/docs/Web/API/Performance/now#security_requirements
const highResCors = {
    'Cross-Origin-Opener-Policy': 'same-origin',
    'Cross-Origin-Embedder-Policy': 'require-corp'
}

http.createServer(async function (request, response) {
    const uri = url.parse(request.url).pathname
    let filename = path.join(process.cwd(), uri)

    try {
        const stats = await fs.stat(filename)
        if (stats.isDirectory()) {
            filename += '/index.html';
        }
    } catch {
        fail404(response)
        return
    }

    let file
    try {
        file = await fs.readFile(filename, 'binary')
    } catch (err) {
        response.writeHead(500, { 'Content-Type': 'text/plain' })
        response.write(err.toString() + '\n')
        response.end()
        return
    }

    let mimeType = mimeTypes[filename.split('.').pop()]
    if (!mimeType) {
        mimeType = 'text/plain';
    }

    response.writeHead(200, { 'Content-Type': mimeType, ...highResCors })
    response.write(file, 'binary')
    response.end()
}).listen(parseInt(port, 10))

function fail404(response) {
    response.writeHead(404, { 'Content-Type': 'text/plain' })
    response.write('404 Not Found\n')
    response.end()
}

console.log('Static file server running at http://localhost:' + port + '/\nCTRL + C to shutdown');
