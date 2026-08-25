import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { ConsoleApp } from './app'
import './styles.css'

export { ConsoleApp }

const root = document.getElementById('root')
if (root) {
  createRoot(root).render(
    <StrictMode>
      <ConsoleApp />
    </StrictMode>,
  )
}
