import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

export function BootstrapNotice() {
  return (
    <main aria-labelledby="bootstrap-title">
      <h1 id="bootstrap-title">BA Agent</h1>
      <p>Console composition is awaiting the approved comp-first design decision.</p>
    </main>
  )
}

const root = document.getElementById('root')
if (root) {
  createRoot(root).render(
    <StrictMode>
      <BootstrapNotice />
    </StrictMode>,
  )
}
