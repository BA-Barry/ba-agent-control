import { render, screen } from '@testing-library/react'
import { expect, test } from 'vitest'

import { BootstrapNotice } from './main'

test('renders the non-designed comp-first bootstrap surface', () => {
  render(<BootstrapNotice />)
  expect(screen.getByRole('heading', { name: 'BA Agent' })).toBeInTheDocument()
  expect(screen.getByText(/awaiting the approved comp-first/i)).toBeInTheDocument()
})
