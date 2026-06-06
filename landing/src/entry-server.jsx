import { renderToString } from 'react-dom/server'
import App from './App.jsx'

// Rendered at build time by prerender.mjs and injected into dist/index.html,
// so crawlers and AI scrapers see the full hero without executing JS.
export function render() {
  return renderToString(<App />)
}
