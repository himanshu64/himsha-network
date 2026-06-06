import { readFileSync, writeFileSync } from 'node:fs'
import { render } from './dist-server/entry-server.js'

// Inject the server-rendered hero into the built index.html so the static HTML
// (what crawlers/AI scrapers see before JS runs) contains the real content.
const indexPath = 'dist/index.html'
const appHtml = render()
const template = readFileSync(indexPath, 'utf8')

if (!template.includes('<div id="root"></div>')) {
  throw new Error('prerender: could not find <div id="root"></div> in dist/index.html')
}

const out = template.replace('<div id="root"></div>', `<div id="root">${appHtml}</div>`)
writeFileSync(indexPath, out)
console.log(`✓ prerendered dist/index.html (${appHtml.length} chars of static markup)`)
