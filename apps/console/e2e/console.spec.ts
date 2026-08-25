import AxeBuilder from '@axe-core/playwright'
import { expect, test, type Page, type Route } from '@playwright/test'

const routes = [
  { path: '/', zh: '项目导线与当前闭合状态', en: 'Project traverse and current closure' },
  { path: '/projects', zh: '项目控制点登记', en: 'Project control-point register' },
  { path: '/workflows', zh: '工作流与任务导线', en: 'Workflow and task traverse' },
  { path: '/runs', zh: '运行详情', en: 'Run detail' },
  { path: '/approvals', zh: '审批中心', en: 'Approval center' },
  { path: '/quality', zh: '质量门与永久收据', en: 'Quality gates and permanent receipts' },
  { path: '/settings', zh: '设置与适配器', en: 'Settings and adapters' },
] as const

const settings = [
  { language: 'zh', theme: 'light' },
  { language: 'zh', theme: 'dark' },
  { language: 'en', theme: 'light' },
  { language: 'en', theme: 'dark' },
] as const

async function seedSettings(page: Page, language: 'zh' | 'en', theme: 'light' | 'dark') {
  await page.addInitScript(({ language, theme }) => {
    window.localStorage.setItem('ba-agent-language', language)
    window.localStorage.setItem('ba-agent-theme', theme)
  }, { language, theme })
}

async function expectNoA11yViolations(page: Page) {
  const result = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'])
    .analyze()
  expect(result.violations, JSON.stringify(result.violations, null, 2)).toEqual([])
}

async function expectNoDocumentOverflow(page: Page) {
  const dimensions = await page.evaluate(() => ({
    clientWidth: document.documentElement.clientWidth,
    scrollWidth: document.documentElement.scrollWidth,
  }))
  expect(dimensions.scrollWidth).toBeLessThanOrEqual(dimensions.clientWidth)
}

function json(route: Route, body: unknown, status = 200) {
  return route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) })
}

async function mockEmptyApi(page: Page, delayMs = 0, runtime?: Record<string, unknown>) {
  await page.route('**/api/v1/**', async (route) => {
    if (delayMs > 0) await new Promise((resolve) => setTimeout(resolve, delayMs))
    const path = new URL(route.request().url()).pathname
    if (path === '/api/v1/session') {
      await json(route, { csrfToken: 'e2e-csrf', origin: 'http://127.0.0.1:4173', expiresInSeconds: 900 })
      return
    }
    if (path === '/api/v1/overview') {
      await json(route, { projects: [], pendingApprovals: 0, runningRuns: 0, failedRuns: 0, orphanedRuns: 0 })
      return
    }
    if (path === '/api/v1/adapters') {
      await json(route, { adapters: runtime ? [{ id: 'codex', capabilities: runtime }] : [] })
      return
    }
    await json(route, [])
  })
}

async function mockDemoFallback(page: Page) {
  await page.route('**/api/v1/**', (route) => json(route, {
    error: { code: 'service_unavailable', message: 'E2E demo fallback' },
  }, 503))
}

async function mockLongRunApi(page: Page) {
  const now = '2026-08-25T06:00:00Z'
  const projectId = '01LONGPROJECT00000000000001'
  const taskId = '01LONGTASK000000000000000001'
  const runId = '01LONGRUN0000000000000000001'
  const longName = `控制点-${'非常长的中英文混合项目名 Long project name 🚧 '.repeat(8)}`
  await page.route('**/api/v1/**', async (route) => {
    const path = new URL(route.request().url()).pathname
    if (path.includes('/events')) {
      await route.abort('connectionrefused')
      return
    }
    if (path === '/api/v1/session') {
      await json(route, { csrfToken: 'e2e-csrf', origin: 'http://127.0.0.1:4173', expiresInSeconds: 900 })
      return
    }
    if (path === '/api/v1/overview') {
      await json(route, {
        projects: [{ id: projectId, name: longName, root_path: `G:/work/${longName}`, description: longName, language: 'zh-CN', created_at: now, updated_at: now }],
        pendingApprovals: 0,
        runningRuns: 1,
        failedRuns: 0,
        orphanedRuns: 0,
      })
      return
    }
    if (path === '/api/v1/agent-profiles') {
      await json(route, [{ id: '01PROFILE', name: 'Implementer', role: 'Implementer', runtime: 'codex' }])
      return
    }
    if (path === '/api/v1/approvals') {
      await json(route, [])
      return
    }
    if (path === '/api/v1/adapters') {
      await json(route, { adapters: [{ id: 'codex', capabilities: {
        available: true, authenticated: true, version: { major: 0, minor: 149, patch: 1 },
        app_server: true, exec_json_fallback: true, approvals: true, resume: true,
        guide: true, cancellation: true, degraded_reason: null,
      } }] })
      return
    }
    if (path === '/api/v1/workstreams') {
      await json(route, [{ id: '01WORKSTREAM', project_id: projectId, name: longName, description: longName, status: 'active' }])
      return
    }
    if (path === '/api/v1/milestones') {
      await json(route, [{
        id: '01MILESTONE', project_id: projectId, workstream_id: '01WORKSTREAM', name: longName,
        description: longName, status: 'active', due_at: null, created_at: now, updated_at: now,
      }])
      return
    }
    if (path === '/api/v1/tasks') {
      await json(route, [{
        id: taskId,
        project_id: projectId,
        workstream_id: '01WORKSTREAM',
        milestone_id: null,
        title: longName,
        description: longName,
        status: 'running',
        write: true,
        path_scope: [`apps/${longName}/**`],
        depends_on: [],
        created_at: now,
        updated_at: now,
      }])
      return
    }
    if (path === '/api/v1/runs') {
      await json(route, [{
        id: runId,
        task_id: taskId,
        agent_profile_id: '01PROFILE',
        status: 'running',
        adapter: 'codex-app-server',
        runtime_run_id: 'runtime-long',
        runtime_thread_id: 'thread-long',
        started_at: now,
        finished_at: null,
        created_at: now,
        updated_at: now,
        event_sequence: 9_999_999,
      }])
      return
    }
    if (path === '/api/v1/worktrees') {
      await json(route, [{ id: '01WORKTREE', project_id: projectId, task_id: taskId, run_id: runId, path: `G:/worktrees/${longName}`, status: 'active' }])
      return
    }
    if (path === '/api/v1/gates' || path === '/api/v1/receipts') {
      await json(route, [])
      return
    }
    await json(route, { error: { code: 'not_found', message: 'Unmocked endpoint' } }, 404)
  })
}

for (const setting of settings) {
  test(`${setting.language}/${setting.theme}: all routes pass accessibility and viewport checks`, async ({ page }) => {
    await seedSettings(page, setting.language, setting.theme)
    await mockDemoFallback(page)
    for (const route of routes) {
      await page.goto(route.path)
      await expect(page.locator('html')).toHaveAttribute('lang', setting.language === 'zh' ? 'zh-CN' : 'en')
      await expect(page.locator('html')).toHaveAttribute('data-theme', setting.theme)
      await expect(page.getByRole('heading', { name: setting.language === 'zh' ? route.zh : route.en }).last()).toBeVisible()
      await expectNoA11yViolations(page)
      await expectNoDocumentOverflow(page)
    }
  })
}

test('keyboard users can skip directly to the main content', async ({ page }) => {
  await seedSettings(page, 'zh', 'light')
  await mockDemoFallback(page)
  await page.goto('/')
  await page.keyboard.press('Tab')
  await expect(page.getByRole('link', { name: '跳到主要内容' })).toBeFocused()
  await page.keyboard.press('Enter')
  await expect(page.locator('#main-content')).toBeFocused()
})

test('settings reports an installed but unauthenticated Codex CLI honestly', async ({ page }) => {
  await mockEmptyApi(page, 0, {
    available: false,
    authenticated: false,
    version: { major: 0, minor: 149, patch: 1 },
    app_server: true,
    exec_json_fallback: true,
    approvals: true,
    resume: true,
    guide: true,
    cancellation: true,
    degraded_reason: 'Codex CLI is not authenticated; run codex login',
  })
  await page.goto('/settings')
  await expect(page.getByText('需要登录 · codex login', { exact: true })).toBeVisible()
  await expect(page.getByText('Codex CLI 尚未登录；请在终端运行 codex login。', { exact: true })).toBeVisible()
})

test('slow and empty local APIs have explicit, honest states', async ({ page }) => {
  await seedSettings(page, 'zh', 'light')
  await mockEmptyApi(page, 700)
  await page.goto('/projects')
  await expect(page.getByRole('status').filter({ hasText: '正在连接本地服务' })).toBeVisible()
  await expect(page.getByRole('heading', { level: 3, name: '项目登记为空' })).toBeVisible()
  await expect(page.getByText('实时数据', { exact: true })).toBeVisible()
  await expectNoA11yViolations(page)
})

test('a live project can register a real milestone from the console', async ({ page }) => {
  await seedSettings(page, 'zh', 'light')
  await mockLongRunApi(page)
  await page.goto('/projects')
  await expect(page.getByText('实时数据', { exact: true })).toBeVisible()
  await page.getByRole('button', { name: '登记里程碑' }).click()
  await page.getByLabel('里程碑名称').fill('个人 Alpha 验收')
  await page.getByLabel('结果与验收边界').fill('连续三次自托管运行无状态丢失')
  await page.getByLabel('目标日期（可选）').fill('2026-08-31')
  const requestPromise = page.waitForRequest((request) =>
    request.method() === 'POST' && new URL(request.url()).pathname === '/api/v1/milestones')
  await page.getByRole('button', { name: '确认里程碑' }).click()
  const request = await requestPromise
  expect(request.postDataJSON()).toMatchObject({
    name: '个人 Alpha 验收',
    status: 'active',
    dueAt: '2026-08-31T23:59:59.000Z',
  })
  await expect(page.getByRole('status').filter({ hasText: '当前里程碑已登记' })).toBeVisible()
  await page.goto('/workflows')
  await page.getByRole('button', { name: '新建任务' }).click()
  await page.getByLabel('前置任务（可多选）').selectOption('01LONGTASK000000000000000001')
  await page.getByLabel('任务标题').fill('验证当前里程碑关联')
  const taskRequestPromise = page.waitForRequest((candidate) =>
    candidate.method() === 'POST' && new URL(candidate.url()).pathname === '/api/v1/tasks')
  await page.getByRole('button', { name: '创建草稿' }).click()
  const taskRequest = await taskRequestPromise
  expect(taskRequest.postDataJSON()).toMatchObject({
    milestoneId: '01MILESTONE',
    dependsOn: ['01LONGTASK000000000000000001'],
  })
  await expect(page.getByRole('status').filter({ hasText: '任务草稿已创建' })).toBeVisible()
  await expectNoA11yViolations(page)
})

test('very long live content stays contained and reports a disconnected event stream', async ({ page }) => {
  await seedSettings(page, 'en', 'dark')
  await mockLongRunApi(page)
  await page.goto('/runs')
  await expect(page.getByText('Live data', { exact: true })).toBeVisible()
  await expect(page.getByRole('button', { name: /EV 9999999/ })).toBeVisible()
  await expect(page.getByText('Event stream disconnected; retrying')).toBeVisible()
  await expectNoDocumentOverflow(page)
  await expectNoA11yViolations(page)
})

test('forced colors and 200% text keep the approval surface operable', async ({ page }) => {
  await seedSettings(page, 'zh', 'light')
  await mockDemoFallback(page)
  await page.emulateMedia({ forcedColors: 'active' })
  await page.addInitScript(() => {
    document.documentElement.style.fontSize = '200%'
  })
  await page.goto('/approvals')
  await expect(page.getByRole('heading', { name: '审批中心' }).last()).toBeVisible()
  await expect(page.getByRole('button', { name: '显示与语言设置' })).toBeVisible()
  await expectNoDocumentOverflow(page)
  await expectNoA11yViolations(page)
})

test('production console stays within LCP, INP, and CLS budgets', async ({ page }) => {
  await seedSettings(page, 'zh', 'light')
  await mockDemoFallback(page)
  await page.addInitScript(() => {
    const vitals = { lcp: 0, inp: 0, cls: 0 }
    Object.defineProperty(window, '__baAgentVitals', { value: vitals, configurable: true })
    new PerformanceObserver((list) => {
      const entry = list.getEntries().at(-1)
      if (entry) vitals.lcp = entry.startTime
    }).observe({ type: 'largest-contentful-paint', buffered: true })
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        const shift = entry as PerformanceEntry & { hadRecentInput?: boolean; value?: number }
        if (!shift.hadRecentInput) vitals.cls += shift.value ?? 0
      }
    }).observe({ type: 'layout-shift', buffered: true })
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        const event = entry as PerformanceEntry & { interactionId?: number; duration: number }
        if ((event.interactionId ?? 0) > 0) vitals.inp = Math.max(vitals.inp, event.duration)
      }
    }).observe({ type: 'event', buffered: true, durationThreshold: 16 })
  })
  await page.goto('/')
  await expect(page.getByRole('heading', { name: '项目导线与当前闭合状态' }).last()).toBeVisible()
  await expect.poll(() => page.evaluate(() => (window as Window & {
    __baAgentVitals: { lcp: number }
  }).__baAgentVitals.lcp)).toBeGreaterThan(0)
  await page.getByRole('button', { name: '显示与语言设置' }).click()
  await page.getByRole('menuitemradio', { name: 'English' }).click()
  await page.waitForTimeout(500)
  const vitals = await page.evaluate(() => (window as Window & {
    __baAgentVitals: { lcp: number; inp: number; cls: number }
  }).__baAgentVitals)
  console.log(`BA Agent Web Vitals: ${JSON.stringify(vitals)}`)
  expect(vitals.lcp).toBeGreaterThan(0)
  expect(vitals.lcp).toBeLessThan(1_500)
  expect(vitals.inp).toBeGreaterThan(0)
  expect(vitals.inp).toBeLessThan(200)
  expect(vitals.cls).toBeLessThan(0.1)
})
