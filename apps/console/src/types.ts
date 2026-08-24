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
  milestone: string
  progress: number
  health: 'healthy' | 'attention' | 'blocked'
  activeRuns: number
  openTasks: number
  receiptHead: string
}

export interface TaskSummary {
  id: string
  code: string
  title: string
  workstream: string
  role: string
  status: TaskStatus
  scope: string
  gate: string
  dependencies: string[]
}

export interface RunSummary {
  id: string
  taskCode: string
  taskTitle: string
  role: string
  runtime: 'app-server' | 'exec-json'
  status: RunStatus
  worktree: string
  elapsed: string
  eventSequence: number
}

export interface ApprovalSummary {
  id: string
  action: 'push' | 'merge' | 'deploy' | 'delete' | 'elevate'
  title: string
  detail: string
  requestedBy: string
  requestedAt: string
  risk: 'medium' | 'high'
}

export interface GateResult {
  id: string
  name: string
  status: 'passed' | 'running' | 'failed' | 'pending'
  duration: string
  command: string
  evidence: string
}

export interface RunEvent {
  sequence: number
  time: string
  kind: 'state' | 'tool' | 'message' | 'gate' | 'approval'
  title: string
  detail: string
}
