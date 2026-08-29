import { demoSnapshot } from '../data/demo'
import type {
  AgentProfileSummary,
  ApprovalSummary,
  ConfiguredGateSummary,
  ConsoleSnapshot,
  GateResult,
  ProjectSummary,
  ReceiptSummary,
  RunEvent,
  RunStatus,
  RunSummary,
  TaskStatus,
  TaskSummary,
} from '../types'

interface SessionResponse {
  csrfToken: string
  origin: string
  expiresInSeconds: number
}

interface OverviewResponse {
  projects: ApiProject[]
  pendingApprovals: number
  runningRuns: number
  failedRuns: number
  orphanedRuns: number
}

interface ApiProject {
  id: string
  name: string
  root_path: string
  description: string
  language: string
  created_at: string
  updated_at: string
}

interface ApiWorkstream {
  id: string
  project_id: string
  name: string
  description: string
  status: string
}

interface ApiMilestone {
  id: string
  project_id: string
  workstream_id: string | null
  name: string
  description: string
  status: 'planned' | 'active' | 'completed' | 'cancelled'
  due_at: string | null
  created_at: string
  updated_at: string
}

interface ApiTask {
  id: string
  project_id: string
  workstream_id: string | null
  milestone_id: string | null
  title: string
  description: string
  status: TaskStatus
  write: boolean
  path_scope: string[]
  depends_on: string[]
  created_at: string
  updated_at: string
}

interface ApiAgentProfile {
  id: string
  name: string
  role: string
  runtime: string
  config?: {
    sandboxMode?: string
  }
}

interface ApiConfiguredGatesResponse {
  projectId: string
  gates: Array<{
    name: string
    program: string
    args: string[]
  }>
  unavailableReason: string | null
}

interface ApiRun {
  id: string
  task_id: string
  agent_profile_id: string | null
  status: RunStatus
  adapter: string
  runtime_run_id: string | null
  runtime_thread_id: string | null
  started_at: string | null
  finished_at: string | null
  created_at: string
  updated_at: string
  event_sequence: number
}

interface ApiWorktree {
  id: string
  project_id: string
  task_id: string
  run_id: string | null
  path: string
  status: string
}

interface ApiApproval {
  id: string
  run_id: string
  kind: string
  status: string
  request: unknown
  requested_at: string
}

interface ApiGate {
  id: string
  task_id: string
  run_id: string | null
  name: string
  status: 'pending' | 'running' | 'passed' | 'failed' | 'skipped'
  command: string | null
  result: unknown
  created_at: string
  completed_at: string | null
}

interface ApiReceipt {
  id: string
  project_id: string
  kind: string
  previous_hash: string | null
  hash: string
  created_at: string
}

interface ApiRunEvent {
  run_id: string
  sequence: number
  kind: string
  payload: unknown
  created_at: string
}

interface ApiRuntimeCapabilities {
  available: boolean
  authenticated: boolean
  version: { major: number; minor: number; patch: number } | null
  app_server: boolean
  exec_json_fallback: boolean
  approvals: boolean
  resume: boolean
  guide: boolean
  cancellation: boolean
  degraded_reason: string | null
}

interface ApiAdaptersResponse {
  adapters: Array<{
    id: string
    capabilities?: ApiRuntimeCapabilities
  }>
}

interface ProjectBundle {
  project: ApiProject
  workstreams: ApiWorkstream[]
  milestones: ApiMilestone[]
  tasks: ApiTask[]
  runs: ApiRun[]
  worktrees: ApiWorktree[]
  gates: ApiGate[]
  configuredGates: ApiConfiguredGatesResponse
  receipts: ApiReceipt[]
}

export class ApiError extends Error {
  constructor(
    message: string,
    public readonly status: number,
    public readonly code?: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    credentials: 'same-origin',
    ...init,
    headers: {
      Accept: 'application/json',
      ...init?.headers,
    },
  })
  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`.trim()
    let code: string | undefined
    try {
      const body = await response.json() as {
        error?: string | { code?: string; message?: string }
        message?: string
        code?: string
      }
      if (body.error && typeof body.error === 'object') {
        message = body.error.message ?? message
        code = body.error.code
      } else {
        message = body.message ?? body.error ?? message
        code = body.code
      }
    } catch {
      // The status line remains useful when an intermediary returns a non-JSON error.
    }
    throw new ApiError(message, response.status, code)
  }
  return response.json() as Promise<T>
}

function query(path: string, key: string, value: string) {
  const params = new URLSearchParams({ [key]: value })
  return `${path}?${params.toString()}`
}

function shortCode(id: string) {
  return `TSK-${id.slice(-5).toUpperCase()}`
}

function shortHash(hash: string | null | undefined) {
  if (!hash) return '—'
  return hash.length > 12 ? `${hash.slice(0, 8)}…${hash.slice(-4)}` : hash
}

function elapsed(start: string | null, finish: string | null) {
  if (!start) return '—'
  const startMs = Date.parse(start)
  const finishMs = finish ? Date.parse(finish) : Date.now()
  if (!Number.isFinite(startMs) || !Number.isFinite(finishMs)) return '—'
  const seconds = Math.max(0, Math.floor((finishMs - startMs) / 1_000))
  const hours = Math.floor(seconds / 3_600)
  const minutes = Math.floor((seconds % 3_600) / 60)
  const remainder = seconds % 60
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m ${String(remainder).padStart(2, '0')}s`
}

function relativeTime(value: string) {
  const timestamp = Date.parse(value)
  if (!Number.isFinite(timestamp)) return { zh: value, en: value }
  const minutes = Math.max(0, Math.floor((Date.now() - timestamp) / 60_000))
  if (minutes < 1) return { zh: '刚刚', en: 'just now' }
  if (minutes < 60) return { zh: `${minutes} 分钟前`, en: `${minutes} minutes ago` }
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return { zh: `${hours} 小时前`, en: `${hours} hours ago` }
  const days = Math.floor(hours / 24)
  return { zh: `${days} 天前`, en: `${days} days ago` }
}

function stableDetail(value: unknown) {
  if (typeof value === 'string') return value
  if (value === null || value === undefined) return '—'
  try {
    return JSON.stringify(value)
  } catch {
    return String(value)
  }
}

function titleFromKind(kind: string) {
  return kind
    .replace(/[._-]+/g, ' ')
    .replace(/\b\w/g, (letter) => letter.toUpperCase())
}

function approvalAction(kind: string): ApprovalSummary['action'] {
  const normalized = kind.toLowerCase()
  if (normalized.includes('push')) return 'push'
  if (normalized.includes('merge')) return 'merge'
  if (normalized.includes('deploy')) return 'deploy'
  if (normalized.includes('delete') || normalized.includes('remove')) return 'delete'
  return 'elevate'
}

function gateEvidence(result: unknown) {
  if (result && typeof result === 'object') {
    const record = result as Record<string, unknown>
    const value = record.summary ?? record.stdout ?? record.message ?? record.error
    if (typeof value === 'string' && value.trim()) return value.trim()
  }
  return stableDetail(result)
}

function gateDuration(gate: ApiGate) {
  if (!gate.completed_at) return gate.status === 'running' ? 'RUNNING' : '—'
  const start = Date.parse(gate.created_at)
  const finish = Date.parse(gate.completed_at)
  if (!Number.isFinite(start) || !Number.isFinite(finish)) return '—'
  return `${Math.max(0, (finish - start) / 1_000).toFixed(1)}s`
}

async function loadProjectBundle(project: ApiProject): Promise<ProjectBundle> {
  const [workstreams, milestones, tasks, runs, worktrees, receipts, configuredGates] = await Promise.all([
    apiFetch<ApiWorkstream[]>(query('/api/v1/workstreams', 'project_id', project.id)),
    apiFetch<ApiMilestone[]>(query('/api/v1/milestones', 'project_id', project.id)),
    apiFetch<ApiTask[]>(query('/api/v1/tasks', 'project_id', project.id)),
    apiFetch<ApiRun[]>(query('/api/v1/runs', 'project_id', project.id)),
    apiFetch<ApiWorktree[]>(query('/api/v1/worktrees', 'project_id', project.id)),
    apiFetch<ApiReceipt[]>(query('/api/v1/receipts', 'project_id', project.id)),
    apiFetch<ApiConfiguredGatesResponse>(query('/api/v1/gates/configured', 'project_id', project.id)),
  ])
  const gates = (await Promise.all(tasks.map((task) =>
    apiFetch<ApiGate[]>(query('/api/v1/gates', 'task_id', task.id)),
  ))).flat()
  return { project, workstreams, milestones, tasks, runs, worktrees, gates, configuredGates, receipts }
}

function mapSnapshot(
  session: SessionResponse,
  overview: OverviewResponse,
  profiles: ApiAgentProfile[],
  approvals: ApiApproval[],
  bundles: ProjectBundle[],
  adapters: ApiAdaptersResponse,
): ConsoleSnapshot {
  const allTasks = bundles.flatMap((bundle) => bundle.tasks)
  const allRuns = bundles.flatMap((bundle) => bundle.runs)
  const allGates = bundles.flatMap((bundle) => bundle.gates)
  const configuredGates: ConfiguredGateSummary[] = bundles.flatMap((bundle) =>
    bundle.configuredGates.gates.map((gate) => ({
      projectId: bundle.project.id,
      name: gate.name,
      command: [gate.program, ...gate.args].join(' '),
    })),
  )
  const gateConfigurationIssues = Object.fromEntries(
    bundles
      .filter((bundle) => Boolean(bundle.configuredGates.unavailableReason))
      .map((bundle) => [bundle.project.id, bundle.configuredGates.unavailableReason as string]),
  )
  const allReceipts = bundles.flatMap((bundle) => bundle.receipts)
  const taskById = new Map(allTasks.map((task) => [task.id, task]))
  const runById = new Map(allRuns.map((run) => [run.id, run]))
  const profileById = new Map(profiles.map((profile) => [profile.id, profile]))
  const workstreamById = new Map(bundles.flatMap((bundle) => bundle.workstreams).map((item) => [item.id, item]))
  const worktreeByRunId = new Map(
    bundles.flatMap((bundle) => bundle.worktrees)
      .filter((item): item is ApiWorktree & { run_id: string } => Boolean(item.run_id))
      .map((item) => [item.run_id, item]),
  )
  const gatesByTask = new Map<string, ApiGate[]>()
  for (const gate of allGates) gatesByTask.set(gate.task_id, [...(gatesByTask.get(gate.task_id) ?? []), gate])

  const tasks: TaskSummary[] = allTasks.map((task) => {
    const workstream = task.workstream_id ? workstreamById.get(task.workstream_id) : undefined
    const run = allRuns.find((candidate) => candidate.task_id === task.id)
    const profile = run?.agent_profile_id ? profileById.get(run.agent_profile_id) : undefined
    const taskGates = gatesByTask.get(task.id) ?? []
    const configuredNames = configuredGates
      .filter((gate) => gate.projectId === task.project_id)
      .map((gate) => gate.name)
    const latestByName = new Map<string, ApiGate>()
    for (const gate of taskGates) latestByName.set(gate.name, gate)
    const passed = configuredNames.filter((name) => {
      const status = latestByName.get(name)?.status
      return status === 'passed' || status === 'skipped'
    }).length
    return {
      id: task.id,
      projectId: task.project_id,
      code: shortCode(task.id),
      title: task.title,
      titleEn: task.title,
      workstream: workstream?.name ?? '未分配工作流',
      workstreamEn: workstream?.name ?? 'Unassigned workstream',
      role: profile?.role ?? profile?.name ?? 'unassigned',
      status: task.status,
      scope: task.write ? (task.path_scope.join(', ') || '—') : 'READ-ONLY',
      gate: `${passed} / ${configuredNames.length}`,
      dependencies: task.depends_on.map((id) => shortCode(id)),
      write: task.write,
    }
  })

  const runs: RunSummary[] = allRuns.map((run) => {
    const task = taskById.get(run.task_id)
    const profile = run.agent_profile_id ? profileById.get(run.agent_profile_id) : undefined
    const worktree = worktreeByRunId.get(run.id)
    return {
      id: run.id,
      taskId: run.task_id,
      taskCode: task ? shortCode(task.id) : shortCode(run.task_id),
      taskTitle: task?.title ?? '未知任务',
      taskTitleEn: task?.title ?? 'Unknown task',
      role: profile?.role ?? profile?.name ?? 'Coordinator',
      runtime: run.adapter.includes('exec') ? 'exec-json' : 'app-server',
      status: run.status,
      worktree: worktree?.path ?? (task?.write ? '待分配' : 'READ-ONLY'),
      threadId: run.runtime_thread_id,
      elapsed: elapsed(run.started_at, run.finished_at),
      eventSequence: run.event_sequence,
    }
  })

  const projectSummaries: ProjectSummary[] = bundles.map((bundle) => {
    const milestone = bundle.milestones.find((item) => item.status === 'active')
      ?? bundle.milestones.find((item) => item.status === 'planned')
      ?? bundle.milestones[0]
    const milestoneTasks = milestone
      ? bundle.tasks.filter((task) => task.milestone_id === milestone.id)
      : bundle.tasks
    const completed = milestoneTasks.filter((task) => task.status === 'completed').length
    const progress = milestoneTasks.length === 0 ? 0 : Math.round(completed / milestoneTasks.length * 100)
    const hasFailure = bundle.tasks.some((task) => task.status === 'blocked' || task.status === 'failed')
    const hasAttention = bundle.tasks.some((task) => ['queued', 'running', 'review'].includes(task.status))
    const projectReceipts = [...bundle.receipts].sort((a, b) => a.created_at.localeCompare(b.created_at))
    return {
      id: bundle.project.id,
      name: bundle.project.name,
      path: bundle.project.root_path,
      branch: 'local',
      milestoneId: milestone?.id ?? null,
      milestone: milestone?.name ?? '暂无里程碑',
      milestoneEn: milestone?.name ?? 'No milestone',
      progress,
      health: hasFailure ? 'blocked' : hasAttention ? 'attention' : 'healthy',
      activeRuns: bundle.runs.filter((run) => ['queued', 'starting', 'running', 'waiting_approval'].includes(run.status)).length,
      openTasks: bundle.tasks.filter((task) => !['completed', 'cancelled'].includes(task.status)).length,
      receiptHead: shortHash(projectReceipts.at(-1)?.hash),
    }
  })

  const approvalSummaries: ApprovalSummary[] = approvals.map((approval) => {
    const run = runById.get(approval.run_id)
    const task = run ? taskById.get(run.task_id) : undefined
    const profile = run?.agent_profile_id ? profileById.get(run.agent_profile_id) : undefined
    const action = approvalAction(approval.kind)
    const relative = relativeTime(approval.requested_at)
    const title = titleFromKind(approval.kind)
    return {
      id: approval.id,
      action,
      title,
      titleEn: title,
      detail: stableDetail(approval.request),
      requestedBy: `${profile?.role ?? profile?.name ?? 'Runtime'} · ${task ? shortCode(task.id) : approval.run_id.slice(-8)}`,
      requestedAt: relative.zh,
      requestedAtEn: relative.en,
      risk: action === 'delete' || action === 'elevate' ? 'high' : 'medium',
    }
  })

  const gates: GateResult[] = allGates.map((gate) => ({
    id: gate.id,
    taskId: gate.task_id,
    runId: gate.run_id,
    name: gate.name,
    nameEn: gate.name,
    status: gate.status === 'skipped' ? 'pending' : gate.status,
    duration: gateDuration(gate),
    command: gate.command ?? '—',
    evidence: gateEvidence(gate.result),
  }))

  const agentProfiles: AgentProfileSummary[] = profiles.map((profile) => ({
    id: profile.id,
    name: profile.name,
    role: profile.role,
    runtime: profile.runtime,
    sandboxMode: profile.config?.sandboxMode ?? 'read-only',
  }))

  const receipts: ReceiptSummary[] = allReceipts.map((receipt) => ({
    id: receipt.id,
    kind: receipt.kind,
    hash: receipt.hash,
    previousHash: receipt.previous_hash,
    createdAt: receipt.created_at,
  }))
  const orderedReceipts = [...receipts].sort((a, b) => a.createdAt.localeCompare(b.createdAt))
  const codex = adapters.adapters?.find((adapter) => adapter.id === 'codex')?.capabilities
  const version = codex?.version

  return {
    source: 'live',
    fallbackReason: null,
    csrfToken: session.csrfToken,
    activeProjectId: projectSummaries[0]?.id ?? null,
    projects: projectSummaries,
    tasks,
    agentProfiles,
    runs,
    approvals: approvalSummaries,
    gates,
    configuredGates,
    gateConfigurationIssues,
    events: [],
    receipts,
    runtime: {
      available: codex?.available ?? false,
      authenticated: codex?.authenticated ?? false,
      version: version ? `${version.major}.${version.minor}.${version.patch}` : null,
      appServer: codex?.app_server ?? false,
      execJsonFallback: codex?.exec_json_fallback ?? false,
      approvals: codex?.approvals ?? false,
      resume: codex?.resume ?? false,
      guide: codex?.guide ?? false,
      cancellation: codex?.cancellation ?? false,
      degradedReason: codex?.degraded_reason ?? null,
    },
    metrics: {
      pendingApprovals: overview.pendingApprovals,
      runningRuns: overview.runningRuns,
      failedRuns: overview.failedRuns,
      orphanedRuns: overview.orphanedRuns,
      receiptCount: receipts.length,
      receiptHead: orderedReceipts.at(-1)?.hash ?? null,
      eventCount: 0,
    },
  }
}

export async function loadConsoleSnapshot(): Promise<ConsoleSnapshot> {
  try {
    const [session, overview, profiles, approvals, adapters] = await Promise.all([
      apiFetch<SessionResponse>('/api/v1/session'),
      apiFetch<OverviewResponse>('/api/v1/overview'),
      apiFetch<ApiAgentProfile[]>('/api/v1/agent-profiles'),
      apiFetch<ApiApproval[]>('/api/v1/approvals'),
      apiFetch<ApiAdaptersResponse>('/api/v1/adapters'),
    ])
    const projects = overview.projects.length > 0
      ? overview.projects
      : await apiFetch<ApiProject[]>('/api/v1/projects')
    const bundles = await Promise.all(projects.map(loadProjectBundle))
    return mapSnapshot(session, overview, profiles, approvals, bundles, adapters)
  } catch (error) {
    const reason = error instanceof Error ? error.message : 'Local service unavailable'
    return { ...demoSnapshot, fallbackReason: reason }
  }
}

async function mutate<T>(path: string, csrfToken: string, body?: unknown): Promise<T> {
  return apiFetch<T>(path, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'x-ba-csrf-token': csrfToken,
    },
    body: JSON.stringify(body ?? {}),
  })
}

export function respondApproval(id: string, decision: 'approved' | 'rejected', csrfToken: string) {
  return mutate(`/api/v1/approvals/${encodeURIComponent(id)}/respond`, csrfToken, {
    decision,
    reason: 'Decided in the local BA Agent console',
  })
}

export function cancelRun(id: string, csrfToken: string) {
  return mutate(`/api/v1/runs/${encodeURIComponent(id)}/cancel`, csrfToken)
}

export function messageRun(id: string, message: string, csrfToken: string) {
  return mutate(`/api/v1/runs/${encodeURIComponent(id)}/message`, csrfToken, { message })
}

export function verifyReceipts(projectId: string, csrfToken: string) {
  return mutate<{ valid: boolean }>('/api/v1/receipts/verify', csrfToken, { project_id: projectId })
}

export function createProject(input: {
  name: string
  rootPath: string
  description: string
  language: string
}, csrfToken: string) {
  return mutate<{ id: string }>('/api/v1/projects', csrfToken, input)
}

export function createWorkstream(input: {
  projectId: string
  name: string
  description: string
}, csrfToken: string) {
  return mutate<{ id: string }>('/api/v1/workstreams', csrfToken, input)
}

export function createMilestone(input: {
  projectId: string
  workstreamId: string | null
  name: string
  description: string
  status: 'planned' | 'active'
  dueAt: string | null
}, csrfToken: string) {
  return mutate<{ id: string }>('/api/v1/milestones', csrfToken, input)
}

export function createTask(input: {
  projectId: string
  workstreamId: string | null
  workstreamName: string | null
  milestoneId: string | null
  title: string
  description: string
  write: boolean
  pathScope: string[]
  dependsOn: string[]
}, csrfToken: string) {
  return mutate<{ id: string }>('/api/v1/tasks', csrfToken, input)
}

export function dispatchTask(taskId: string, agentProfileId: string, csrfToken: string) {
  return mutate<{ run: { id: string }; dispatch: { id: string } }>('/api/v1/dispatches', csrfToken, {
    taskId,
    agentProfileId,
  })
}

export function runConfiguredGate(
  taskId: string,
  runId: string,
  gate: string,
  csrfToken: string,
) {
  return mutate<{ gate: ApiGate; finalization: unknown }>('/api/v1/gates/run', csrfToken, {
    taskId,
    runId,
    gate,
  })
}

export function retryRun(id: string, csrfToken: string) {
  return mutate<{ mode: 'resumed' | 'retried'; run: { id: string } }>(`/api/v1/runs/${encodeURIComponent(id)}/retry`, csrfToken)
}

export function mapRunEvent(event: ApiRunEvent): RunEvent {
  const normalized = event.kind.toLowerCase()
  const kind: RunEvent['kind'] = normalized.includes('approval') || normalized.includes('permission')
    ? 'approval'
    : normalized.includes('gate')
      ? 'gate'
      : normalized.includes('tool')
        ? 'tool'
        : normalized.includes('message') || normalized.includes('output')
          ? 'message'
          : 'state'
  const title = titleFromKind(event.kind)
  const detail = stableDetail(event.payload)
  const timestamp = new Date(event.created_at)
  return {
    sequence: event.sequence,
    time: Number.isNaN(timestamp.valueOf()) ? event.created_at : timestamp.toLocaleTimeString(undefined, { hour12: false }),
    kind,
    title,
    titleEn: title,
    detail,
    detailEn: detail,
  }
}

export type EventStreamState = 'connecting' | 'connected' | 'disconnected'

export function subscribeRunEvents(
  runId: string,
  after: number,
  onEvent: (event: RunEvent) => void,
  onState: (state: EventStreamState) => void,
) {
  onState('connecting')
  const params = new URLSearchParams({ after: String(after) })
  const source = new EventSource(`/api/v1/runs/${encodeURIComponent(runId)}/events?${params.toString()}`)
  source.onopen = () => onState('connected')
  source.addEventListener('run-event', (message) => {
    try {
      onEvent(mapRunEvent(JSON.parse((message as MessageEvent<string>).data) as ApiRunEvent))
    } catch {
      onState('disconnected')
    }
  })
  source.onerror = () => onState('disconnected')
  return () => source.close()
}
