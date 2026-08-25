import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test } from 'vitest'

import { ConsoleApp } from './main'

beforeEach(() => {
  localStorage.clear()
  window.history.replaceState(null, '', '/')
})

afterEach(() => {
  cleanup()
})

test('renders the approved field-book overview with live operating data', async () => {
  const view = render(<ConsoleApp />)
  expect(await within(view.container).findByRole('heading', { name: '项目导线与当前闭合状态' })).toBeInTheDocument()
  expect(within(view.container).getByRole('heading', { name: '导线记录' })).toBeInTheDocument()
  expect(within(view.container).getByRole('heading', { name: '审批与闭合差' })).toBeInTheDocument()
  expect(within(view.container).getByText('示例数据')).toBeInTheDocument()
  expect(within(view.container).getByText(/收据链连续/)).toBeInTheDocument()
})

test('switches the console language without losing navigation', async () => {
  const user = userEvent.setup()
  const view = render(<ConsoleApp />)
  await user.click(await within(view.container).findByRole('button', { name: '显示与语言设置' }))
  await user.click(screen.getByRole('menuitemradio', { name: 'English' }))
  expect(await within(view.container).findByRole('heading', { name: 'Project traverse and current closure' })).toBeInTheDocument()
  expect(within(view.container).getByRole('navigation', { name: 'Primary navigation' })).toBeInTheDocument()
})

test('keeps system-generated operating records localized across every route', async () => {
  localStorage.setItem('ba-agent-language', 'en')
  const user = userEvent.setup()
  const view = render(<ConsoleApp />)
  const root = within(view.container)

  expect(await root.findByText('SQLite migration and integrity recovery')).toBeInTheDocument()
  expect(root.queryByText('SQLite 迁移与完整性恢复')).not.toBeInTheDocument()

  await user.click(root.getByRole('link', { name: /^Projects 03$/ }))
  expect(await root.findByRole('heading', { name: 'Project control-point register' })).toBeInTheDocument()
  expect(root.getByText('Search and citations')).toBeInTheDocument()

  await user.click(root.getByRole('link', { name: /^Workflows & tasks 12$/ }))
  expect(await root.findByRole('heading', { name: 'Workflow and task traverse' })).toBeInTheDocument()
  expect(root.getByText('Path-boundary property tests')).toBeInTheDocument()

  await user.click(root.getByRole('link', { name: /^Run detail 04$/ }))
  expect(await root.findByRole('heading', { name: 'Run detail', level: 2 })).toBeInTheDocument()
  expect(root.getByText('Changed-scope tests started')).toBeInTheDocument()

  await user.click(root.getByRole('link', { name: /^Approvals 02$/ }))
  expect(await root.findByRole('heading', { name: 'Approval center' })).toBeInTheDocument()
  expect(root.getByText('Push the runtime-handshake branch')).toBeInTheDocument()

  await user.click(root.getByRole('link', { name: /^Quality & receipts 06$/ }))
  expect(await root.findByRole('heading', { name: 'Quality gates and permanent receipts' })).toBeInTheDocument()
  expect(root.getByText('Changed-scope tests')).toBeInTheDocument()

  await user.click(root.getByRole('link', { name: /^Settings & adapters ST$/ }))
  expect(await root.findByRole('heading', { name: 'Settings and adapters' })).toBeInTheDocument()
  expect(root.getByText('Local boundary')).toBeInTheDocument()
})

test('selects the synthetic QA station from the same station source', async () => {
  const user = userEvent.setup()
  const view = render(<ConsoleApp />)
  const root = within(view.container)
  await user.click(await root.findByRole('link', { name: /^总览 OV$/ }))
  await user.click(await root.findByRole('button', { name: /QA-011/ }))
  expect(root.getByText('QA-011 · 收据链前向验证')).toBeInTheDocument()
  expect(root.getByText((content) => content.includes('crates/ba-receipt/**'))).toBeInTheDocument()
})

test('explains demo-only actions instead of presenting inert controls', async () => {
  const user = userEvent.setup()
  const view = render(<ConsoleApp />)
  const root = within(view.container)
  await user.click(await root.findByRole('link', { name: /^项目 03$/ }))
  await user.click(await root.findByRole('button', { name: '注册项目' }))
  expect(await root.findByRole('status')).toHaveTextContent('演示模式：尚未写入本地项目登记。')
})
