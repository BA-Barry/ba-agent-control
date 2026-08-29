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

const draftTaskId = '01DRAFTTASK0000000000000001'
const reviewTaskId = '01REVIEWTASK000000000000001'
const reviewRunId = '01REVIEWRUN0000000000000001'

const settings = [
  { language: 'zh', theme: 'light', visualTheme: 'fieldbook' },
  { language: 'zh', theme: 'dark', visualTheme: 'fieldbook' },
  { language: 'en', theme: 'light', visualTheme: 'fieldbook' },
  { language: 'en', theme: 'dark', visualTheme: 'fieldbook' },
  { language: 'zh', theme: 'light', visualTheme: 'chartroom' },
  { language: 'zh', theme: 'dark', visualTheme: 'chartroom' },
  { language: 'en', theme: 'light', visualTheme: 'chartroom' },
  { language: 'en', theme: 'dark', visualTheme: 'chartroom' },
  { language: 'zh', theme: 'light', visualTheme: 'signalroom' },
  { language: 'zh', theme: 'dark', visualTheme: 'signalroom' },
  { language: 'en', theme: 'light', visualTheme: 'signalroom' },
  { language: 'en', theme: 'dark', visualTheme: 'signalroom' },
] as const

async function seedSettings(page: Page, language: 'zh' | 'en', theme: 'light' | 'dark', visualTheme: 'fieldbook' | 'chartroom' | 'signalroom' = 'fieldbook') {
  await page.addInitScript(({ language, theme, visualTheme }) => {
    window.localStorage.setItem('ba-agent-language', language)
    window.localStorage.setItem('ba-agent-theme', theme)
    window.localStorage.setItem('ba-agent-visual-theme', visualTheme)
  }, { language, theme, visualTheme })
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
      await json(route, [
        { id: '01PROFILE', name: 'Implementer', role: 'Implementer', runtime: 'codex', config: { sandboxMode: 'workspace-write' } },
        { id: '01REVIEWER', name: 'Reviewer', role: 'Reviewer', runtime: 'codex', config: { sandboxMode: 'read-only' } },
      ])
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
      }, {
        id: draftTaskId,
        project_id: projectId,
        workstream_id: '01WORKSTREAM',
        milestone_id: null,
        title: '实现写入范围隔离',
        description: '在独立 worktree 中实现改动',
        status: 'draft',
        write: true,
        path_scope: ['apps/console/**'],
        depends_on: [],
        created_at: now,
        updated_at: now,
      }, {
        id: reviewTaskId,
        project_id: projectId,
        workstream_id: '01WORKSTREAM',
        milestone_id: null,
        title: '执行质量门确认',
        description: '运行项目配置的检查',
        status: 'review',
        write: false,
        path_scope: [],
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
      }, {
        id: reviewRunId,
        task_id: reviewTaskId,
        agent_profile_id: '01REVIEWER',
        status: 'succeeded',
        adapter: 'codex-app-server',
        runtime_run_id: 'runtime-review',
        runtime_thread_id: 'thread-review',
        started_at: now,
        finished_at: now,
        created_at: now,
        updated_at: now,
        event_sequence: 12,
      }])
      return
    }
    if (path === '/api/v1/worktrees') {
      await json(route, [{ id: '01WORKTREE', project_id: projectId, task_id: taskId, run_id: runId, path: `G:/worktrees/${longName}`, status: 'active' }])
      return
    }
    if (path === '/api/v1/gates/configured') {
      await json(route, {
        projectId,
        gates: [{ name: 'format', program: 'cargo', args: ['fmt', '--all', '--', '--check'] }],
        unavailableReason: null,
      })
      return
    }
    if (path === '/api/v1/gates' || path === '/api/v1/receipts') {
      await json(route, [])
      return
    }
    if (path === '/api/v1/dispatches' && route.request().method() === 'POST') {
      await json(route, { run: { id: '01DISPATCHRUN' }, dispatch: { id: '01DISPATCH' } }, 201)
      return
    }
    if (path === '/api/v1/gates/run' && route.request().method() === 'POST') {
      await json(route, {
        gate: { id: '01GATE', task_id: reviewTaskId, run_id: reviewRunId, name: 'format', status: 'passed', command: 'cargo fmt --all -- --check', result: {}, created_at: now, completed_at: now },
        finalization: null,
      }, 201)
      return
    }
    await json(route, { error: { code: 'not_found', message: 'Unmocked endpoint' } }, 404)
  })
}

for (const setting of settings) {
  test(`${setting.language}/${setting.theme}/${setting.visualTheme}: all routes pass accessibility and viewport checks`, async ({ page }) => {
    await seedSettings(page, setting.language, setting.theme, setting.visualTheme)
    await mockDemoFallback(page)
    for (const route of routes) {
      await page.goto(route.path)
      await expect(page.locator('html')).toHaveAttribute('lang', setting.language === 'zh' ? 'zh-CN' : 'en')
      await expect(page.locator('html')).toHaveAttribute('data-theme', setting.theme)
      await expect(page.locator('html')).toHaveAttribute('data-visual-theme', setting.visualTheme)
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

test('display settings supports keyboard selection and Escape focus return', async ({ page }) => {
  await seedSettings(page, 'zh', 'light')
  await mockDemoFallback(page)
  await page.goto('/')
  const trigger = page.getByRole('button', { name: '显示与语言设置' })
  await trigger.focus()
  await trigger.press('ArrowDown')
  await expect(page.getByRole('dialog', { name: '显示与语言' })).toBeVisible()
  await expect(page.getByRole('radio', { name: '中文' })).toBeFocused()
  const englishOption = page.getByRole('radio', { name: 'English' })
  await englishOption.focus()
  await englishOption.press('Space')
  await expect(page.locator('html')).toHaveAttribute('lang', 'en')
  const englishTrigger = page.getByRole('button', { name: 'Display and language settings' })
  await expect(englishTrigger).toHaveAttribute('aria-expanded', 'false')
  await expect(englishTrigger).toBeFocused()
  await englishTrigger.press('Enter')
  await expect(page.getByRole('dialog', { name: 'Display and language' })).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(englishTrigger).toHaveAttribute('aria-expanded', 'false')
  await expect(englishTrigger).toBeFocused()
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
  await expect(page.getByText('实时数据', { exact: true })).toBeVisible({ timeout: 10_000 })
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
  await expectNoA11yViolations(page)
})

test('a live task draft preserves milestone and dependency associations', async ({ page }) => {
  await seedSettings(page, 'zh', 'light')
  await mockLongRunApi(page)
  await page.goto('/workflows')
  await expect(page.getByText('实时数据', { exact: true })).toBeVisible()
  await expect(page.getByRole('heading', { name: '工作流与任务导线' }).last()).toBeVisible()
  await page.getByRole('button', { name: '新建任务' }).click()
  const dependencySelect = page.getByLabel('前置任务（可多选）')
  await expect(dependencySelect).toBeVisible()
  await dependencySelect.selectOption('01LONGTASK000000000000000001')
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

test('a write task dispatch keeps role permissions explicit before launch', async ({ page }) => {
  await seedSettings(page, 'zh', 'light')
  await mockLongRunApi(page)
  await page.goto('/workflows')
  const row = page.getByRole('row').filter({ hasText: '实现写入范围隔离' })
  await row.getByRole('button', { name: '派发' }).click()
  await expect(page.getByRole('heading', { name: /派发/ }).last()).toBeVisible()
  const role = page.getByLabel('Agent 角色')
  await expect(role).toHaveValue('01PROFILE')
  await expect(role.locator('option')).toHaveCount(1)
  await expect(page.getByText('写入范围：apps/console/**。将创建独立 worktree。')).toBeVisible()
  const dispatchRequest = page.waitForRequest((candidate) => candidate.method() === 'POST' && new URL(candidate.url()).pathname === '/api/v1/dispatches')
  await page.getByRole('button', { name: '确认派发' }).click()
  expect((await dispatchRequest).postDataJSON()).toEqual({ taskId: draftTaskId, agentProfileId: '01PROFILE' })
  await expect(page.getByRole('status').filter({ hasText: '任务已派发' })).toBeVisible()
  await expectNoDocumentOverflow(page)
  await expectNoA11yViolations(page)
})

test('a review task can run the configured project gate from the console', async ({ page }) => {
  await seedSettings(page, 'zh', 'light')
  await mockLongRunApi(page)
  await page.goto('/quality')
  await expect(page.getByLabel('检查任务')).toHaveValue(reviewTaskId)
  await expect(page.getByText('cargo fmt --all -- --check')).toBeVisible()
  const gateRequest = page.waitForRequest((candidate) => candidate.method() === 'POST' && new URL(candidate.url()).pathname === '/api/v1/gates/run')
  await page.getByRole('button', { name: '执行', exact: true }).click()
  expect((await gateRequest).postDataJSON()).toEqual({ taskId: reviewTaskId, runId: reviewRunId, gate: 'format' })
  await expect(page.getByRole('status').filter({ hasText: 'format 已完成' })).toBeVisible()
  await expectNoDocumentOverflow(page)
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

for (const visualTheme of ['fieldbook', 'chartroom', 'signalroom'] as const) {
  test(`${visualTheme}: forced colors and 200% text keep the approval surface operable`, async ({ page }) => {
    await seedSettings(page, 'zh', 'light', visualTheme)
    await mockDemoFallback(page)
    await page.emulateMedia({ forcedColors: 'active' })
    await page.addInitScript(() => {
      document.documentElement.style.fontSize = '200%'
    })
    await page.goto('/approvals')
    await expect(page.getByRole('heading', { name: '审批中心' }).last()).toBeVisible()
    await expect(page.getByRole('button', { name: '显示与语言设置' })).toBeVisible()
    const sidebarColors = await page.evaluate(() => {
      const sidebar = document.querySelector<HTMLElement>('.sidebar')
      const inactiveNav = document.querySelector<HTMLElement>('.nav-link:not(.is-active)')
      const inactiveLabel = inactiveNav?.querySelector<HTMLElement>('span')
      const activeNav = document.querySelector<HTMLElement>('.nav-link.is-active')
      const activeLabel = activeNav?.querySelector<HTMLElement>('span')
      const activeCount = activeNav?.querySelector<HTMLElement>('small')
      const activeIcon = activeNav?.querySelector<SVGElement>('svg')
      if (!sidebar || !inactiveNav || !inactiveLabel || !activeNav || !activeLabel || !activeCount || !activeIcon) {
        throw new Error('Sidebar navigation is missing')
      }
      return {
        sidebarBackground: getComputedStyle(sidebar).backgroundColor,
        inactiveLink: getComputedStyle(inactiveNav).color,
        inactiveLabel: getComputedStyle(inactiveLabel).color,
        activeBackground: getComputedStyle(activeNav).backgroundColor,
        activeLink: getComputedStyle(activeNav).color,
        activeLabel: getComputedStyle(activeLabel).color,
        activeCount: getComputedStyle(activeCount).color,
        activeIcon: getComputedStyle(activeIcon).color,
      }
    })
    expect(sidebarColors.inactiveLink).not.toBe(sidebarColors.sidebarBackground)
    expect(sidebarColors.inactiveLabel).not.toBe(sidebarColors.sidebarBackground)
    expect(sidebarColors.activeBackground).not.toBe(sidebarColors.sidebarBackground)
    expect(sidebarColors.activeLink).toBe(sidebarColors.activeLabel)
    expect(sidebarColors.activeLink).toBe(sidebarColors.activeCount)
    expect(sidebarColors.activeLink).toBe(sidebarColors.activeIcon)
    await expectNoDocumentOverflow(page)
    await expectNoA11yViolations(page)
  })
}

test('production console stays within LCP, INP, and CLS budgets', { tag: '@performance' }, async ({ page }) => {
  const cpuRate = Number.parseFloat(process.env.BA_E2E_CPU_RATE ?? '1')
  await seedSettings(page, 'zh', 'light')
  await mockDemoFallback(page)
  await page.addInitScript(() => {
    const vitals = { lcp: 0, inp: 0, cls: 0, interactions: [] as Array<{ name: string; duration: number; target: string }> }
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
        const event = entry as PerformanceEntry & { interactionId?: number; duration: number; target?: EventTarget | null }
        if ((event.interactionId ?? 0) > 0) {
          vitals.inp = Math.max(vitals.inp, event.duration)
          const target = event.target instanceof Element ? event.target : null
          vitals.interactions.push({
            name: event.name,
            duration: event.duration,
            target: target?.getAttribute('aria-label') ?? target?.textContent?.trim().slice(0, 48) ?? '',
          })
        }
      }
    }).observe({ type: 'event', buffered: true, durationThreshold: 16 })
  })
  await page.goto('/')
  await expect(page.getByRole('heading', { name: '项目导线与当前闭合状态' }).last()).toBeVisible()
  await expect.poll(() => page.evaluate(() => (window as Window & {
    __baAgentVitals: { lcp: number }
  }).__baAgentVitals.lcp)).toBeGreaterThan(0)
  if (Number.isFinite(cpuRate) && cpuRate > 1) {
    const session = await page.context().newCDPSession(page)
    await session.send('Emulation.setCPUThrottlingRate', { rate: cpuRate })
  }
  await page.getByRole('button', { name: '显示与语言设置' }).click()
  await page.getByRole('radio', { name: 'English' }).click()
  await page.waitForTimeout(500)
  const vitals = await page.evaluate(() => (window as Window & {
    __baAgentVitals: { lcp: number; inp: number; cls: number; interactions: Array<{ name: string; duration: number; target: string }> }
  }).__baAgentVitals)
  console.log(`BA Agent Web Vitals: ${JSON.stringify(vitals)}`)
  expect(vitals.lcp).toBeGreaterThan(0)
  expect(vitals.lcp).toBeLessThan(1_500)
  expect(vitals.inp).toBeGreaterThan(0)
  expect(vitals.inp).toBeLessThan(200)
  expect(vitals.cls).toBeLessThan(0.1)
})
