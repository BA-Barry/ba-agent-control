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
  status: TaskStatus
  scope: string
  gate: string
  dependencies: string[]
  write: boolean
}

export interface AgentProfileSummary {
  id: string
  name: string
  role: string
  runtime: string
  sandboxMode: 'read-only' | 'workspace-write' | string
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
