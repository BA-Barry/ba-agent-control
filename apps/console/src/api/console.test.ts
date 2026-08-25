import { afterEach, expect, test, vi } from 'vitest'

import { createMilestone, createProject, createTask, loadConsoleSnapshot, mapRunEvent, retryRun, subscribeRunEvents } from './console'

function json(body: unknown, status = 200) {
  return Promise.resolve(new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  }))
}

afterEach(() => {
  vi.unstubAllGlobals()
})

test('maps the authenticated local API into a live console snapshot', async () => {
  const project = {
    id: '01PROJECT00000000000000001', name: 'live-repository', root_path: 'G:\\live',
    description: 'Alpha milestone', language: 'zh-CN', created_at: '2026-08-24T10:00:00Z', updated_at: '2026-08-24T10:00:00Z',
  }
  const task = {
    id: '01TASK00000000000000000001', project_id: project.id, workstream_id: '01WORKSTREAM0000000000001',
    milestone_id: '01MILESTONE00000000000001', title: 'Connect local API', description: '', status: 'running', write: true,
    path_scope: ['apps/console/**'], depends_on: [], created_at: '2026-08-24T10:00:00Z', updated_at: '2026-08-24T10:00:00Z',
  }
  const run = {
    id: '01RUN000000000000000000001', task_id: task.id, agent_profile_id: '01PROFILE0000000000000001',
    status: 'running', adapter: 'codex-app-server', runtime_run_id: 'runtime-1', runtime_thread_id: 'thread-1',
    event_sequence: 17,
    started_at: '2026-08-24T10:00:00Z', finished_at: null, created_at: '2026-08-24T10:00:00Z', updated_at: '2026-08-24T10:00:00Z',
  }
  const fetchMock = vi.fn((input: RequestInfo | URL) => {
    const url = String(input)
    if (url === '/api/v1/session') return json({ csrfToken: 'csrf', origin: 'http://127.0.0.1:41415', expiresInSeconds: 28800 })
    if (url === '/api/v1/overview') return json({ projects: [project], pendingApprovals: 1, runningRuns: 1, failedRuns: 0, orphanedRuns: 0 })
    if (url === '/api/v1/agent-profiles') return json([{ id: run.agent_profile_id, name: 'Implementer', role: 'Implementer', runtime: 'codex' }])
    if (url === '/api/v1/approvals') return json([{ id: '01APPROVAL000000000000001', run_id: run.id, kind: 'git.push', status: 'pending', request: { branch: 'ba/live' }, requested_at: '2026-08-24T10:01:00Z' }])
    if (url.startsWith('/api/v1/workstreams?')) return json([{ id: task.workstream_id, project_id: project.id, name: 'Console', description: '', status: 'active' }])
    if (url.startsWith('/api/v1/milestones?')) return json([{
      id: task.milestone_id, project_id: project.id, workstream_id: task.workstream_id,
      name: 'Personal Alpha', description: 'Real milestone', status: 'active', due_at: '2026-08-31T10:00:00Z',
      created_at: '2026-08-24T10:00:00Z', updated_at: '2026-08-24T10:00:00Z',
    }])
    if (url.startsWith('/api/v1/tasks?')) return json([task])
    if (url.startsWith('/api/v1/runs?')) return json([run])
    if (url.startsWith('/api/v1/worktrees?')) return json([{
      id: '01WORKTREE000000000000001', project_id: project.id, task_id: task.id,
      run_id: run.id, path: 'G:\\BA Agent Data\\worktrees\\live-001', status: 'active',
    }])
    if (url.startsWith('/api/v1/receipts?')) return json([{ id: '01RECEIPT000000000000001', project_id: project.id, kind: 'gate', previous_hash: null, hash: 'abcdef1234567890', created_at: '2026-08-24T10:02:00Z' }])
    if (url.startsWith('/api/v1/gates?')) return json([{ id: '01GATE0000000000000000001', task_id: task.id, run_id: run.id, name: 'lint', status: 'passed', command: 'pnpm lint', result: { summary: '0 warnings' }, created_at: '2026-08-24T10:00:00Z', completed_at: '2026-08-24T10:00:03Z' }])
    return json({ message: `unexpected request: ${url}` }, 404)
  })
  vi.stubGlobal('fetch', fetchMock)

  const snapshot = await loadConsoleSnapshot()

  expect(snapshot.source).toBe('live')
  expect(snapshot.fallbackReason).toBeNull()
  expect(snapshot.csrfToken).toBe('csrf')
  expect(snapshot.projects[0]).toMatchObject({
    name: 'live-repository', milestone: 'Personal Alpha', progress: 0,
    activeRuns: 1, receiptHead: 'abcdef12…7890',
  })
  expect(snapshot.tasks[0]).toMatchObject({ title: 'Connect local API', workstream: 'Console', role: 'Implementer', gate: '1 / 1' })
  expect(snapshot.runs[0]).toMatchObject({
    runtime: 'app-server',
    worktree: 'G:\\BA Agent Data\\worktrees\\live-001',
    threadId: 'thread-1',
    eventSequence: 17,
  })
  expect(snapshot.approvals[0]).toMatchObject({ action: 'push', risk: 'medium' })
  expect(snapshot.metrics).toMatchObject({ pendingApprovals: 1, runningRuns: 1, receiptCount: 1, receiptHead: 'abcdef1234567890' })
})

test('falls back explicitly when the local service is unavailable', async () => {
  vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')))

  const snapshot = await loadConsoleSnapshot()

  expect(snapshot.source).toBe('demo')
  expect(snapshot.fallbackReason).toBe('Failed to fetch')
  expect(snapshot.projects.length).toBeGreaterThan(0)
})

test('normalizes persisted SSE events for the bilingual event register', () => {
  const event = mapRunEvent({
    run_id: '01RUN', sequence: 7, kind: 'approval.requested',
    payload: { target: 'git push' }, created_at: '2026-08-24T10:00:00Z',
  })

  expect(event).toMatchObject({ sequence: 7, kind: 'approval', title: 'Approval Requested' })
  expect(event.detail).toContain('git push')
})

test('reports SSE connection and disconnection while accepting the stable event envelope', () => {
  class FakeEventSource {
    static current: FakeEventSource
    onopen: ((event: Event) => void) | null = null
    onerror: ((event: Event) => void) | null = null
    listeners = new Map<string, (event: MessageEvent<string>) => void>()
    closed = false

    constructor(public readonly url: string) {
      FakeEventSource.current = this
    }

    addEventListener(type: string, listener: EventListenerOrEventListenerObject) {
      this.listeners.set(type, listener as (event: MessageEvent<string>) => void)
    }

    close() {
      this.closed = true
    }
  }
  vi.stubGlobal('EventSource', FakeEventSource)
  const states: string[] = []
  const received: number[] = []

  const unsubscribe = subscribeRunEvents('01RUN', 4, (event) => received.push(event.sequence), (state) => states.push(state))
  const source = FakeEventSource.current
  source.onopen?.(new Event('open'))
  source.listeners.get('run-event')?.(new MessageEvent('run-event', { data: JSON.stringify({
    run_id: '01RUN', sequence: 5, kind: 'runtime.started', payload: {}, created_at: '2026-08-24T10:00:00Z',
  }) }))
  source.onerror?.(new Event('error'))
  unsubscribe()

  expect(source.url).toContain('after=4')
  expect(received).toEqual([5])
  expect(states).toEqual(['connecting', 'connected', 'disconnected'])
  expect(source.closed).toBe(true)
})

test('sends CSRF-protected project creation and retry mutations', async () => {
  const fetchMock = vi.fn()
    .mockImplementationOnce(() => json({ id: '01PROJECT' }, 201))
    .mockImplementationOnce(() => json({ id: '01MILESTONE' }, 201))
    .mockImplementationOnce(() => json({ id: '01TASK' }, 201))
    .mockImplementationOnce(() => json({ mode: 'retried', run: { id: '01RUN2' } }, 202))
  vi.stubGlobal('fetch', fetchMock)

  await createProject({ name: 'Local', rootPath: 'G:\\Local', description: '', language: 'zh-CN' }, 'csrf-token')
  await createMilestone({
    projectId: '01PROJECT', workstreamId: null, name: 'Alpha', description: '',
    status: 'active', dueAt: '2026-08-31T23:59:59Z',
  }, 'csrf-token')
  await createTask({
    projectId: '01PROJECT', workstreamId: null, workstreamName: 'Console', milestoneId: '01MILESTONE',
    title: 'Atomic task', description: '', write: true, pathScope: ['apps/console/**'], dependsOn: ['01DEPENDENCY'],
  }, 'csrf-token')
  await retryRun('01RUN1', 'csrf-token')

  expect(fetchMock).toHaveBeenNthCalledWith(1, '/api/v1/projects', expect.objectContaining({
    method: 'POST',
    credentials: 'same-origin',
    headers: expect.objectContaining({ 'x-ba-csrf-token': 'csrf-token' }),
  }))
  expect(fetchMock).toHaveBeenNthCalledWith(2, '/api/v1/milestones', expect.objectContaining({
    method: 'POST',
    headers: expect.objectContaining({ 'x-ba-csrf-token': 'csrf-token' }),
  }))
  expect(fetchMock).toHaveBeenNthCalledWith(3, '/api/v1/tasks', expect.objectContaining({
    method: 'POST',
    headers: expect.objectContaining({ 'x-ba-csrf-token': 'csrf-token' }),
  }))
  expect(JSON.parse(fetchMock.mock.calls[2][1].body as string)).toMatchObject({
    workstreamName: 'Console', milestoneId: '01MILESTONE', pathScope: ['apps/console/**'],
  })
  expect(fetchMock).toHaveBeenNthCalledWith(4, '/api/v1/runs/01RUN1/retry', expect.objectContaining({
    method: 'POST',
    headers: expect.objectContaining({ 'x-ba-csrf-token': 'csrf-token' }),
  }))
  expect(JSON.parse(fetchMock.mock.calls[0][1].body as string)).toMatchObject({ name: 'Local', rootPath: 'G:\\Local' })
})
