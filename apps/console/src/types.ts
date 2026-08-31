export type TaskStatus =
  | 'draft'
  | 'ready'
  | 'queued'
  | 'running'
  | 'review'
  | 'completed'
  | 'blocked'
  | 'failed'
  | 'cancelled'

export type RunStatus =
  | 'queued'
  | 'starting'
  | 'running'
  | 'waiting_approval'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'orphaned'

export interface ProjectSummary {
  id: string
  name: string
  path: string
  branch: string
  milestoneId: string | null
  milestone: string
  milestoneEn: string
  progress: number
  health: 'healthy' | 'attention' | 'blocked'
  activeRuns: number
  openTasks: number
  receiptHead: string
}

export interface TaskSummary {
  id: string
  projectId: string
  code: string
  title: string
  titleEn: string
  workstream: string
  workstreamEn: string
  role: string
  agentProfileId: string | null
  status: TaskStatus
  scope: string
  gate: string
  dependencies: string[]
  write: boolean
}

export type TaskDispositionAction = 'archived' | 'superseded' | 'restored'

export interface TaskDispositionSummary {
  id: string
  taskId: string
  action: TaskDispositionAction
  reason: string
  replacementTaskId: string | null
  previousDispositionId: string | null
  createdAt: string
}

export interface InactiveTaskSummary {
  id: string
  projectId: string
  title: string
  status: TaskStatus
  write: boolean
  scope: string
  disposition: TaskDispositionSummary
}

export interface WorkflowTemplateTask {
  key: string
  title_zh: string
  title_en: string
  role: string
  write: boolean
  depends_on: string[]
  path_scope: string[]
  acceptance_zh: string[]
  acceptance_en: string[]
}

export interface WorkflowTemplateSummary {
  version: number
  id: string
  name_zh: string
  name_en: string
  description_zh: string
  description_en: string
  tasks: WorkflowTemplateTask[]
  origin: 'builtin' | 'project'
}

export interface WorkflowTemplateCatalog {
  templates: WorkflowTemplateSummary[]
  diagnostics: Array<{ path: string; error: string }>
}

export type ReviewRepairLoopStatus =
  | 'running'
  | 'awaiting_decision'
  | 'confirming'
  | 'completed'
  | 'blocked'
  | 'exhausted'

export interface ReviewRepairLoopSummary {
  id: string
  project_id: string
  workstream_id: string
  title: string
  objective: string
  write_scope: string[]
  max_repairs: number
  repairs_started: number
  status: ReviewRepairLoopStatus
  active_review_task_id: string
  confirmation_task_id: string | null
  decisions: Array<{
    decision: 'accept' | 'repair' | 'block'
    review_task_id: string
    note: string
    decided_at: string
  }>
  created_at: string
  updated_at: string
}

export interface HandoffSummary {
  handoff: {
    id: string
    projectId: string
    sourceTaskId: string
    sourceRunId: string | null
    destinationTaskIds: string[]
    outcome: string
    decisions: string[]
    evidence: string[]
    changedPaths: string[]
    commands: Array<{ command: string; result: string }>
    risks: string[]
    nextSteps: string[]
    artifactIds: string[]
    receiptIds: string[]
    createdAt: string
  }
  artifact: { id: string; path: string; digest: string | null }
}

export interface AgentProfileSummary {
  id: string
  name: string
  role: string
  runtime: string
  sandboxMode: 'read-only' | 'workspace-write' | string
}

export interface QueueSummary {
  dispatchId: string
  runId: string
  taskId: string
  projectId: string
  taskCode: string
  title: string
  titleEn: string
  position: number
  write: boolean
  dependencies: string[]
}

export interface ProjectConcurrencySummary {
  projectId: string
  requestedMaxConcurrency: number
  effectiveMaxConcurrency: number
  cappedByGlobal: boolean
}

export interface SchedulerPolicySummary {
  globalMaxConcurrency: number
  projects: ProjectConcurrencySummary[]
}

export interface RunSummary {
  id: string
  taskId: string
  taskCode: string
  taskTitle: string
  taskTitleEn: string
  role: string
  runtime: 'app-server' | 'exec-json'
  status: RunStatus
  worktree: string
  threadId: string | null
  elapsed: string
  eventSequence: number
}

export interface ApprovalSummary {
  id: string
  action: 'push' | 'merge' | 'deploy' | 'delete' | 'elevate'
  title: string
  titleEn: string
  detail: string
  requestedBy: string
  requestedAt: string
  requestedAtEn: string
  risk: 'medium' | 'high'
}

export interface GateResult {
  id: string
  taskId: string
  runId: string | null
  name: string
  nameEn: string
  status: 'passed' | 'running' | 'failed' | 'pending'
  duration: string
  command: string
  evidence: string
}

export interface ConfiguredGateSummary {
  projectId: string
  name: string
  command: string
}

export interface RunEvent {
  sequence: number
  time: string
  kind: 'state' | 'tool' | 'message' | 'gate' | 'approval'
  title: string
  titleEn: string
  detail: string
  detailEn: string
}

export interface ReceiptSummary {
  id: string
  kind: string
  hash: string
  previousHash: string | null
  createdAt: string
}

export interface ConsoleMetrics {
  pendingApprovals: number
  runningRuns: number
  failedRuns: number
  orphanedRuns: number
  receiptCount: number
  receiptHead: string | null
  eventCount: number
}

export interface RuntimeStatus {
  available: boolean
  authenticated: boolean
  version: string | null
  appServer: boolean
  execJsonFallback: boolean
  approvals: boolean
  resume: boolean
  guide: boolean
  cancellation: boolean
  degradedReason: string | null
}

export interface ConsoleSnapshot {
  source: 'live' | 'demo'
  fallbackReason: string | null
  csrfToken: string | null
  activeProjectId: string | null
  projects: ProjectSummary[]
  tasks: TaskSummary[]
  agentProfiles: AgentProfileSummary[]
  queue: QueueSummary[]
  scheduler: SchedulerPolicySummary
  runs: RunSummary[]
  approvals: ApprovalSummary[]
  gates: GateResult[]
  configuredGates: ConfiguredGateSummary[]
  gateConfigurationIssues: Record<string, string>
  events: RunEvent[]
  receipts: ReceiptSummary[]
  runtime: RuntimeStatus
  metrics: ConsoleMetrics
}
