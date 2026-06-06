import React from 'react'
import { hydrateRoot } from 'react-dom/client'
import App from './App.jsx'
import './index.css'

// The markup is prerendered into dist/index.html at build time; hydrate over it.
hydrateRoot(
  document.getElementById('root'),
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
