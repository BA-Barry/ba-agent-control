import * as RadioGroup from '@radix-ui/react-radio-group'
import {
  ArrowRight,
  CheckCircle,
  ClipboardText,
  Clock,
  Code,
  Folders,
  GitBranch,
  Globe,
  Moon,
  Pause,
  Play,
  Pulse,
  SealCheck,
  SlidersHorizontal,
  SquaresFour,
  Sun,
  WarningCircle,
  XCircle,
} from '@phosphor-icons/react'
import { QueryClient, QueryClientProvider, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  Link,
  Outlet,
  RouterProvider,
  createRootRoute,
  createRoute,
  createRouter,
  useRouterState,
} from '@tanstack/react-router'
import {
  createColumnHelper,
  flexRender,
  getCoreRowModel,
  useReactTable,
} from '@tanstack/react-table'
import clsx from 'clsx'
import {
  useCallback,
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  useTransition,
  type ReactNode,
} from 'react'

import {
  cancelRun,
  createMilestone,
  createProject,
  createTask,
  dispatchTask,
  loadConsoleSnapshot,
  messageRun,
  respondApproval,
  retryRun,
  runConfiguredGate,
  subscribeRunEvents,
  verifyReceipts,
  type EventStreamState,
} from './api/console'
import { demoSnapshot } from './data/demo'
import type { AgentProfileSummary, ApprovalSummary, ConsoleSnapshot, RunEvent, RunStatus, TaskStatus, TaskSummary } from './types'

type Language = 'zh' | 'en'
type ThemePreference = 'system' | 'light' | 'dark'
type VisualTheme = 'fieldbook' | 'chartroom' | 'signalroom'

interface UiSettings {
  language: Language
  setLanguage: (language: Language) => void
  theme: ThemePreference
  setTheme: (theme: ThemePreference) => void
  visualTheme: VisualTheme
  setVisualTheme: (visualTheme: VisualTheme) => void
}

const UiSettingsContext = createContext<UiSettings | null>(null)

function useUiSettings() {
  const value = useContext(UiSettingsContext)
  if (!value) throw new Error('UiSettingsContext is missing')
  return value
}

function useText() {
  const { language } = useUiSettings()
  return (zh: string, en: string) => (language === 'zh' ? zh : en)
}

const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: Number.POSITIVE_INFINITY, retry: false } },
})

function useConsoleSnapshot() {
  const query = useQuery({
    queryKey: ['console-snapshot'],
    queryFn: loadConsoleSnapshot,
    placeholderData: demoSnapshot,
  })
  return { data: query.data ?? demoSnapshot, isLoading: query.isPlaceholderData }
}

const navItems = [
  { to: '/', zh: '总览', en: 'Overview', icon: SquaresFour, count: 'OV' },
  { to: '/projects', zh: '项目', en: 'Projects', icon: Folders, count: '03' },
  { to: '/workflows', zh: '工作流与任务', en: 'Workflows & tasks', icon: GitBranch, count: '12' },
  { to: '/runs', zh: '运行详情', en: 'Run detail', icon: Pulse, count: '04' },
  { to: '/approvals', zh: '审批中心', en: 'Approvals', icon: ClipboardText, count: '02' },
  { to: '/quality', zh: '质量与收据', en: 'Quality & receipts', icon: SealCheck, count: '06' },
  { to: '/settings', zh: '设置与适配器', en: 'Settings & adapters', icon: SlidersHorizontal, count: 'ST' },
] as const

const roleCopy: Record<string, { zh: string; en: string; detailZh: string; detailEn: string }> = {
  coordinator: { zh: '协调员', en: 'Coordinator', detailZh: '核对依赖、工作图和需要用户决定的事项；保持只读。', detailEn: 'Checks dependencies, the work graph, and decisions that need the user; remains read-only.' },
  planner: { zh: '规划员', en: 'Planner', detailZh: '拆分可执行任务、验收标准、依赖与验证步骤；保持只读。', detailEn: 'Defines executable tasks, acceptance criteria, dependencies, and verification; remains read-only.' },
  explorer: { zh: '探索员', en: 'Explorer', detailZh: '追踪真实调用路径并提供文件证据；保持只读。', detailEn: 'Traces real execution paths and returns file evidence; remains read-only.' },
  implementer: { zh: '实现者', en: 'Implementer', detailZh: '在声明范围内完成最小且完整的代码修改。', detailEn: 'Makes the smallest complete code change inside the declared scope.' },
  reviewer: { zh: '审查员', en: 'Reviewer', detailZh: '按行为和风险优先审查，给出可复现证据；保持只读。', detailEn: 'Reviews behavior and risk with reproducible evidence; remains read-only.' },
  test_engineer: { zh: '测试工程师', en: 'Test Engineer', detailZh: '验证可观察行为和失败路径，可在声明范围内补充测试。', detailEn: 'Tests observable behavior and failure paths and may add tests inside scope.' },
  security_reviewer: { zh: '安全审查员', en: 'Security Reviewer', detailZh: '检查权限、输入、文件系统和凭据风险；保持只读。', detailEn: 'Inspects authorization, inputs, filesystem, and credential risk; remains read-only.' },
  docs_reviewer: { zh: '文档审查员', en: 'Docs Reviewer', detailZh: '逐项验证文档命令、恢复和安全说明；保持只读。', detailEn: 'Verifies documented commands, recovery, and safety guidance; remains read-only.' },
}

const gateCopy: Record<string, { zh: string; en: string }> = {
  format: { zh: '格式检查', en: 'Format check' },
  lint: { zh: 'Lint', en: 'Lint' },
  typecheck: { zh: '类型检查', en: 'Type check' },
  changed_tests: { zh: '变更范围测试', en: 'Changed-scope tests' },
  secret_scan: { zh: '秘密扫描', en: 'Secret scan' },
  diff_review: { zh: 'Diff 审查', en: 'Diff review' },
}

function profileCopy(profile: AgentProfileSummary) {
  return roleCopy[profile.role] ?? {
    zh: profile.name,
    en: profile.name,
    detailZh: profile.role,
    detailEn: profile.role,
  }
}

function recommendProfile(task: TaskSummary, profiles: AgentProfileSummary[]) {
  const signal = `${task.title} ${task.workstream}`.toLowerCase()
  const preferredRole = task.write
    ? /测试|test|qa/.test(signal) ? 'test_engineer' : 'implementer'
    : /安全|security|权限|credential/.test(signal) ? 'security_reviewer'
      : /文档|说明|docs|documentation|readme/.test(signal) ? 'docs_reviewer'
        : /审查|review|audit/.test(signal) ? 'reviewer'
          : /计划|拆分|plan|design/.test(signal) ? 'planner'
            : /协调|依赖|coordinate/.test(signal) ? 'coordinator'
              : 'explorer'
  return profiles.find((profile) => profile.role === preferredRole)
    ?? profiles.find((profile) => !task.write || profile.sandboxMode === 'workspace-write')
    ?? profiles[0]
}

function DisplaySettingsControl() {
  const text = useText()
  const { language, setLanguage, theme, setTheme, visualTheme, setVisualTheme } = useUiSettings()
  const [open, setOpen] = useState(false)
  const controlRef = useRef<HTMLDivElement>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    if (!open) return
    const onPointerDown = (event: PointerEvent) => {
      if (!controlRef.current?.contains(event.target as Node)) setOpen(false)
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      setOpen(false)
      triggerRef.current?.focus()
    }
    document.addEventListener('pointerdown', onPointerDown)
    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      document.removeEventListener('keydown', onKeyDown)
    }
  }, [open])

  const choose = (apply: () => void) => {
    apply()
    setOpen(false)
    window.requestAnimationFrame(() => triggerRef.current?.focus())
  }

  return (
    <div className="settings-control" ref={controlRef}>
      <button
        ref={triggerRef}
        type="button"
        className="icon-button"
        aria-label={text('显示与语言设置', 'Display and language settings')}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls="display-settings-panel"
        title={text('显示与语言', 'Display and language')}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={(event) => {
          if (event.key !== 'ArrowDown') return
          event.preventDefault()
          setOpen(true)
          window.requestAnimationFrame(() => controlRef.current?.querySelector<HTMLButtonElement>('[role="radio"]')?.focus())
        }}
      >
        <SlidersHorizontal size={19} aria-hidden="true" />
      </button>
      <div
        id="display-settings-panel"
        className="settings-menu"
        role="dialog"
        aria-label={text('显示与语言', 'Display and language')}
        hidden={!open}
      >
        <div className="settings-label" id="language-settings-label">{text('语言', 'Language')}</div>
        <RadioGroup.Root
          className="settings-radio-group"
          aria-labelledby="language-settings-label"
          value={language}
          onValueChange={(value) => choose(() => setLanguage(value as Language))}
        >
          <RadioGroup.Item className="settings-choice" value="zh"><span aria-hidden="true" />中文<RadioGroup.Indicator>●</RadioGroup.Indicator></RadioGroup.Item>
          <RadioGroup.Item className="settings-choice" value="en"><span aria-hidden="true" />English<RadioGroup.Indicator>●</RadioGroup.Indicator></RadioGroup.Item>
        </RadioGroup.Root>
        <div className="settings-separator" role="separator" />
        <div className="settings-label" id="theme-settings-label">{text('主题', 'Theme')}</div>
        <RadioGroup.Root
          className="settings-radio-group"
          aria-labelledby="theme-settings-label"
          value={theme}
          onValueChange={(value) => choose(() => setTheme(value as ThemePreference))}
        >
          <RadioGroup.Item className="settings-choice" value="system"><Globe size={15} aria-hidden="true" />{text('跟随系统', 'System')}<RadioGroup.Indicator>●</RadioGroup.Indicator></RadioGroup.Item>
          <RadioGroup.Item className="settings-choice" value="light"><Sun size={15} aria-hidden="true" />{text('浅色', 'Light')}<RadioGroup.Indicator>●</RadioGroup.Indicator></RadioGroup.Item>
          <RadioGroup.Item className="settings-choice" value="dark"><Moon size={15} aria-hidden="true" />{text('深色', 'Dark')}<RadioGroup.Indicator>●</RadioGroup.Indicator></RadioGroup.Item>
        </RadioGroup.Root>
        <div className="settings-separator" role="separator" />
        <div className="settings-label" id="visual-theme-settings-label">{text('界面主题', 'Interface theme')}</div>
        <RadioGroup.Root
          className="settings-radio-group"
          aria-labelledby="visual-theme-settings-label"
          value={visualTheme}
          onValueChange={(value) => choose(() => setVisualTheme(value as VisualTheme))}
        >
          <RadioGroup.Item className="settings-choice" value="fieldbook"><span className="theme-mark theme-mark-fieldbook" aria-hidden="true" />{text('外业簿', 'Field book')}<RadioGroup.Indicator>●</RadioGroup.Indicator></RadioGroup.Item>
          <RadioGroup.Item className="settings-choice" value="chartroom"><span className="theme-mark theme-mark-chartroom" aria-hidden="true" />{text('航图台', 'Chart room')}<RadioGroup.Indicator>●</RadioGroup.Indicator></RadioGroup.Item>
          <RadioGroup.Item className="settings-choice" value="signalroom"><span className="theme-mark theme-mark-signalroom" aria-hidden="true" />{text('信号台', 'Signal room')}<RadioGroup.Indicator>●</RadioGroup.Indicator></RadioGroup.Item>
        </RadioGroup.Root>
      </div>
    </div>
  )
}

function AppShell() {
  const text = useText()
  const { data, isLoading } = useConsoleSnapshot()
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const activeItem = navItems.find((item) => item.to === pathname) ?? navItems[0]
  const activeProject = data.projects[0]
  const navCount = (to: typeof navItems[number]['to'], fallback: string) => {
    if (data.source === 'demo') return fallback
    if (to === '/projects') return String(data.projects.length).padStart(2, '0')
    if (to === '/workflows') return String(data.tasks.length).padStart(2, '0')
    if (to === '/runs') return String(data.runs.length).padStart(2, '0')
    if (to === '/approvals') return String(data.approvals.length).padStart(2, '0')
    if (to === '/quality') return String(data.gates.length).padStart(2, '0')
    return fallback
  }
  const pageTitle = pathname === '/'
    ? text('项目导线与当前闭合状态', 'Project traverse and current closure')
    : text(activeItem.zh, activeItem.en)

  return (
    <div className="app-shell">
      <a className="skip-link" href="#main-content">
        {text('跳到主要内容', 'Skip to main content')}
      </a>
      <aside className="sidebar" aria-label={text('应用侧栏', 'Application sidebar')}>
        <div className="brand-block">
          <strong>BA Agent</strong>
          <span>LOCAL CONTROL PLANE · α.1</span>
        </div>
        <nav className="primary-nav" aria-label={text('主导航', 'Primary navigation')}>
          {navItems.map(({ to, zh, en, icon: Icon, count }) => (
            <Link
              key={to}
              to={to}
              activeOptions={{ exact: to === '/' }}
              activeProps={{ className: 'is-active' }}
              className="nav-link"
            >
              <Icon aria-hidden="true" size={17} weight="regular" />
              <span>{text(zh, en)}</span>
              <small>{navCount(to, count)}</small>
            </Link>
          ))}
        </nav>
        <section className="control-index" aria-labelledby="control-index-title">
          <p className="measure-label" id="control-index-title">
            {text('项目控制点', 'Project control points')}
          </p>
          {data.projects.map((project, index) => (
            <Link
              to="/projects"
              key={project.id}
              className={clsx('control-project', index === 0 && 'is-selected')}
            >
              <span className="control-symbol" aria-hidden="true" />
              <span>
                <strong>{project.name}</strong>
                <small className="mono">
                  CP-0{index + 1} · {project.branch} · {project.progress}%
                </small>
              </span>
            </Link>
          ))}
        </section>
        <div className={clsx('sidebar-foot', data.source === 'demo' && 'is-offline')}>
          <span className="live-dot" aria-hidden="true" />
          <span><strong>{data.source === 'live' ? text('本地服务已连接', 'Local service connected') : text('演示降级模式', 'Demo fallback mode')}</strong>{data.fallbackReason && <small title={data.fallbackReason}>{data.fallbackReason}</small>}</span>
        </div>
      </aside>

      <div className="workspace">
        <header className="topbar">
          <div>
            <span className="topbar-context">{text('BA Agent / 本地工作面', 'BA Agent / local workspace')}</span>
            <h1>{pageTitle}</h1>
          </div>
          <div className="topbar-datum">
            <span><small>{text('里程碑', 'Milestone')}</small><strong>{activeProject ? text(activeProject.milestone, activeProject.milestoneEn) : '—'}</strong></span>
            <span><small>{text('并发运行', 'Active runs')}</small><strong className="accent-brass mono">{data.metrics.runningRuns} / 4</strong></span>
            <span className="topbar-status"><small>{text('数据源', 'Data source')}</small><strong><i className="live-dot" />{data.source === 'live' ? 'LOCAL API' : 'DEMO'}</strong></span>
          </div>
          <DisplaySettingsControl />
        </header>
        <main id="main-content" tabIndex={-1}>
          {isLoading && (
            <div className="connection-banner" role="status" aria-live="polite">
              <Clock size={15} aria-hidden="true" />
              <span>
                <strong>{text('正在连接本地服务', 'Connecting to the local service')}</strong>
                {text('当前内容是明确标记的示例占位，连接完成后会自动替换。', 'The current content is a clearly marked demo placeholder and will be replaced after connection.')}
              </span>
            </div>
          )}
          <Outlet />
        </main>
      </div>
    </div>
  )
}

const taskStatusCopy: Record<TaskStatus, [string, string]> = {
  draft: ['草稿', 'Draft'], ready: ['就绪', 'Ready'], queued: ['排队', 'Queued'], running: ['运行中', 'Running'],
  review: ['审查中', 'Review'], completed: ['已完成', 'Completed'], blocked: ['阻塞', 'Blocked'], failed: ['失败', 'Failed'], cancelled: ['已取消', 'Cancelled'],
}

const runStatusCopy: Record<RunStatus, [string, string]> = {
  queued: ['排队', 'Queued'], starting: ['启动中', 'Starting'], running: ['运行中', 'Running'], waiting_approval: ['等待审批', 'Waiting approval'],
  succeeded: ['成功', 'Succeeded'], failed: ['失败', 'Failed'], cancelled: ['已取消', 'Cancelled'], orphaned: ['失联', 'Orphaned'],
}

function StatusText({ status, kind = 'task' }: { status: TaskStatus | RunStatus; kind?: 'task' | 'run' }) {
  const { language } = useUiSettings()
  const copy = kind === 'run' ? runStatusCopy[status as RunStatus] : taskStatusCopy[status as TaskStatus]
  return <span className={clsx('status-text', `status-${status}`)}><i aria-hidden="true" />{copy[language === 'zh' ? 0 : 1]}</span>
}

function SourceMark({ snapshot }: { snapshot: ConsoleSnapshot }) {
  const text = useText()
  return <span
    className={clsx('verification-stamp', snapshot.source === 'live' && 'is-live')}
    title={snapshot.fallbackReason ?? undefined}
  >{snapshot.source === 'live' ? text('实时数据', 'Live data') : text('示例数据', 'Demo data')}</span>
}

function EmptyRegister({ titleZh, titleEn, detailZh, detailEn }: {
  titleZh: string
  titleEn: string
  detailZh: string
  detailEn: string
}) {
  const text = useText()
  return <section className="empty-register" role="status"><span className="control-symbol" aria-hidden="true" /><div><h3>{text(titleZh, titleEn)}</h3><p>{text(detailZh, detailEn)}</p></div></section>
}

function DemoActionButton({ labelZh, labelEn, messageZh, messageEn, primary = false }: {
  labelZh: string
  labelEn: string
  messageZh: string
  messageEn: string
  primary?: boolean
}) {
  const text = useText()
  const [feedbackVisible, setFeedbackVisible] = useState(false)
  return <span className="demo-action-group">
    <button type="button" className={clsx('field-action', primary && 'primary')} onClick={() => setFeedbackVisible(true)}>{text(labelZh, labelEn)}</button>
    {feedbackVisible && <span className="demo-action-feedback" role="status"><CheckCircle size={14} aria-hidden="true" />{text(messageZh, messageEn)}</span>}
  </span>
}

function OverviewPage() {
  const text = useText()
  const { data } = useConsoleSnapshot()
  const [selectedTask, setSelectedTask] = useState(data.tasks[0]?.code ?? '')
  const qaTask: TaskSummary = {
    id: '01JBTASK000000000000000011',
    projectId: data.projects[0]?.id ?? '',
    code: 'QA-011',
    title: '收据链前向验证',
    titleEn: 'Forward receipt-chain verification',
    workstream: '质量与收据',
    workstreamEn: 'Quality and receipts',
    role: 'Test Engineer',
    status: 'running',
    scope: 'crates/ba-receipt/**',
    gate: '2 / 6',
    dependencies: ['CORE-014'],
    write: true,
  }
  const stationPositions = [
    { left: '20%', top: '25%' }, { left: '38%', top: '42%' }, { left: '56%', top: '30%' },
    { left: '54%', top: '66%' }, { left: '75%', top: '48%' },
  ]
  const stationTasks = data.source === 'demo' ? [...data.tasks.slice(0, 4), qaTask] : data.tasks.slice(0, 5)
  const stations = stationTasks.map((task, index) => ({ task, ...stationPositions[index] }))
  const selected = stations.find(({ task }) => task.code === selectedTask)?.task ?? stations[0]?.task
  const activeProject = data.projects[0]
  useEffect(() => {
    if (stations.length > 0 && !stations.some(({ task }) => task.code === selectedTask)) setSelectedTask(stations[0].task.code)
  }, [selectedTask, stations])

  if (!activeProject || !selected) {
    return <section className="register-page"><PageHeader titleZh="项目导线与当前闭合状态" titleEn="Project traverse and current closure" descriptionZh="从注册本地 Git 仓库开始建立任务导线。" descriptionEn="Register a local Git repository to establish the first task traverse." /><EmptyRegister titleZh="尚未注册项目" titleEn="No project registered" detailZh="使用 CLI 的 project add 注册第一个仓库；刷新后会自动显示实时数据。" detailEn="Register the first repository with the CLI project add command; live data appears after refresh." /></section>
  }

  return (
    <section className="overview-page" aria-label={text('项目导线总览', 'Project traverse overview')}>
      <div className="overview-main">
        <section className="traverse-field" aria-labelledby="traverse-heading">
          <div className="traverse-heading">
            <p className="measure-label">TRAVERSE / {data.metrics.eventCount} EVENTS</p>
            <h2 id="traverse-heading">{text('导线记录', 'Traverse record')}</h2>
            <div className="traverse-meta"><span className="mono">CONTROL CP-01 · DATUM {activeProject.branch} · SCALE 1:1</span><SourceMark snapshot={data} /></div>
          </div>
          <span className="north-mark mono" aria-hidden="true">N ↑ E 121°28′</span>
          <div className="traverse-canvas">
            <svg viewBox="0 0 900 520" preserveAspectRatio="none" aria-hidden="true">
              <path className="guide-route" d="M40 460 L118 96 L282 252 L436 118 L610 290 L758 180 L860 420" />
              <path className="main-route" d="M40 460 L118 96 L282 252 L436 118 L610 290 L758 180 L860 420" />
              <path className="branch-route" d="M282 252 L428 430 L610 290" />
            </svg>
            <div className="benchmark-station" style={{ left: '8%', top: '76%' }}>
              <span className="station-pin" />
              <div className="station-label"><b className="mono">BM · CP-01</b><strong>{text(activeProject.milestone, activeProject.milestoneEn)}</strong><small className="mono">{activeProject.progress}% · {activeProject.openTasks} OPEN</small></div>
            </div>
            {stations.map(({ task, left, top }) => (
              <button
                key={task.code}
                type="button"
                style={{ left, top }}
                className={clsx('task-station', selectedTask === task.code && 'is-selected', task.status === 'blocked' && 'is-blocked')}
                aria-pressed={selectedTask === task.code}
                onClick={() => setSelectedTask(task.code)}
              >
                <span className="station-pin" aria-hidden="true" />
                <span className={clsx('station-label', task.code === 'SEC-006' && 'station-label-left')}>
                  <b className="mono">{task.code}</b>
                  <strong>{text(task.title, task.titleEn)}</strong>
                  <small><StatusText status={task.status} /> <span className="mono">GATE {task.gate}</span></small>
                </span>
              </button>
            ))}
            <div className="benchmark-station closure-station" style={{ left: '89%', top: '68%' }}>
              <span className="station-pin" />
              <div className="station-label station-label-left"><b className="mono">CLOSE-01</b><strong>{text('质量闭合候选', 'Quality closure candidate')}</strong><small className="mono">Δ 0.034 · WAITING</small></div>
            </div>
          </div>
        </section>

        <section className="event-register" aria-labelledby="event-register-title">
          <header><h2 id="event-register-title">{text('事件与质量证据登记', 'Event and quality evidence register')}</h2><span className="mono">APPEND-ONLY · {data.events.length > 0 ? `SEQ ${data.events.at(-1)?.sequence}—${data.events[0]?.sequence}` : 'NO EVENTS'}</span></header>
          <div className="event-table-wrap" tabIndex={0} role="region" aria-label={text('事件登记表', 'Event register table')}>
            <table>
              <thead><tr><th>{text('时间', 'Time')}</th><th>{text('序号', 'Sequence')}</th><th>{text('类型', 'Kind')}</th><th>{text('记录', 'Record')}</th></tr></thead>
              <tbody>{data.events.slice(0, 4).map((event) => <tr key={event.sequence}><td className="mono">{event.time}</td><td className="mono">EV {event.sequence}</td><td className="mono">{event.kind} / {text(event.title, event.titleEn)}</td><td>{text(event.detail, event.detailEn)}</td></tr>)}</tbody>
            </table>
          </div>
        </section>
      </div>

      <aside className="action-ledger" aria-labelledby="action-ledger-title">
        <p className="measure-label">RIGHT MARGIN / ACTION</p>
        <h2 id="action-ledger-title">{text('审批与闭合差', 'Approvals and closure error')}</h2>
        {data.approvals.map((approval) => <ApprovalSlip key={approval.id} approval={approval} />)}
        <section className="closure-summary">
          <p className="measure-label">{text('闭合差', 'CLOSURE ERROR')} / TOL ≤ 0.050</p>
          <div><strong className="mono">0.034</strong><span className="verification-stamp">{text('范围内', 'Within range')}</span></div>
          <p>{text(`${data.gates.filter((gate) => gate.status === 'passed').length} / ${data.gates.length} 质量门已完成。`, `${data.gates.filter((gate) => gate.status === 'passed').length} of ${data.gates.length} gates are complete.`)}</p>
        </section>
        <section className="selected-station-summary" aria-live="polite">
          <p className="measure-label">{text('当前测站', 'Selected station')}</p>
          <strong>{selected.code} · {text(selected.title, selected.titleEn)}</strong>
          <span className="mono">{selected.scope} · GATE {selected.gate}</span>
        </section>
      </aside>

      <nav className="mobile-action-rail" aria-label={text('移动端快捷操作', 'Mobile quick action')}>
        <span><small>{text('当前测站', 'Selected station')}</small><strong className="mono">{selected.code}</strong></span>
        <Link to="/runs" className="field-action primary">{text('打开运行', 'Open run')}<ArrowRight size={15} /></Link>
      </nav>

      <footer className="receipt-footer">
        <div className="receipt-chain-mark" aria-hidden="true"><i /><span /><i /></div>
        <div><strong>{text(`收据链连续 · ${data.metrics.eventCount} 个事件已排序`, `Receipt chain continuous · ${data.metrics.eventCount} events sequenced`)}</strong><span className="mono">HEAD {data.metrics.receiptHead ? `${data.metrics.receiptHead.slice(0, 8)}…${data.metrics.receiptHead.slice(-4)}` : '—'} · SHA-256</span></div>
        <Link to="/runs" className="field-action primary">{text('打开当前运行', 'Open active run')}<ArrowRight size={16} /></Link>
      </footer>
    </section>
  )
}

function ApprovalSlip({ approval }: { approval: ApprovalSummary }) {
  const text = useText()
  return (
    <article className="approval-slip">
      <header><h3>{text(approval.title, approval.titleEn)}</h3><span className="verification-stamp">{approval.risk === 'high' ? text('高风险', 'High risk') : text('中风险', 'Medium risk')}</span></header>
      <p className="mono">{approval.requestedBy}<br />{approval.detail}<br />{text(approval.requestedAt, approval.requestedAtEn)}</p>
      <div><Link to="/approvals" className="field-action primary">{text('审阅并决定', 'Review and decide')}</Link><Link to="/approvals" className="field-action">{text('前往拒绝', 'Go to reject')}</Link></div>
    </article>
  )
}

function PageHeader({ titleZh, titleEn, descriptionZh, descriptionEn, action }: { titleZh: string; titleEn: string; descriptionZh: string; descriptionEn: string; action?: ReactNode }) {
  const text = useText()
  const { data } = useConsoleSnapshot()
  return <header className="page-header"><div><h2>{text(titleZh, titleEn)}</h2><p>{text(descriptionZh, descriptionEn)}</p></div><div className="page-actions"><SourceMark snapshot={data} />{action}</div></header>
}

function ProjectsPage() {
  const text = useText()
  const { data } = useConsoleSnapshot()
  const queryCache = useQueryClient()
  const [formOpen, setFormOpen] = useState(false)
  const [milestoneFormOpen, setMilestoneFormOpen] = useState(false)
  const [name, setName] = useState('')
  const [rootPath, setRootPath] = useState('')
  const [description, setDescription] = useState('')
  const [milestoneProjectId, setMilestoneProjectId] = useState(data.activeProjectId ?? '')
  const [milestoneName, setMilestoneName] = useState('')
  const [milestoneDescription, setMilestoneDescription] = useState('')
  const [milestoneDueAt, setMilestoneDueAt] = useState('')
  const [feedback, setFeedback] = useState('')
  useEffect(() => {
    if (data.projects.length > 0 && !data.projects.some((project) => project.id === milestoneProjectId)) setMilestoneProjectId(data.projects[0].id)
  }, [data.projects, milestoneProjectId])
  const createMutation = useMutation({
    mutationFn: () => {
      if (!data.csrfToken) throw new Error('Live session is unavailable')
      return createProject({ name, rootPath, description, language: 'zh-CN' }, data.csrfToken)
    },
    onSuccess: async () => {
      setFeedback(text('项目已注册。', 'Project registered.'))
      setName('')
      setRootPath('')
      setDescription('')
      setFormOpen(false)
      await queryCache.invalidateQueries({ queryKey: ['console-snapshot'] })
    },
    onError: (error) => setFeedback(error instanceof Error ? error.message : text('项目注册失败。', 'Project registration failed.')),
  })
  const createMilestoneMutation = useMutation({
    mutationFn: () => {
      if (!data.csrfToken || !milestoneProjectId) throw new Error('Live project session is unavailable')
      return createMilestone({
        projectId: milestoneProjectId,
        workstreamId: null,
        name: milestoneName,
        description: milestoneDescription,
        status: 'active',
        dueAt: milestoneDueAt ? new Date(`${milestoneDueAt}T23:59:59Z`).toISOString() : null,
      }, data.csrfToken)
    },
    onSuccess: async () => {
      setFeedback(text('当前里程碑已登记。', 'Current milestone registered.'))
      setMilestoneName('')
      setMilestoneDescription('')
      setMilestoneDueAt('')
      setMilestoneFormOpen(false)
      await queryCache.invalidateQueries({ queryKey: ['console-snapshot'] })
    },
    onError: (error) => setFeedback(error instanceof Error ? error.message : text('里程碑登记失败。', 'Milestone registration failed.')),
  })
  const action = data.source === 'demo'
    ? <><DemoActionButton labelZh="登记里程碑" labelEn="Add milestone" messageZh="演示模式：尚未写入真实里程碑。" messageEn="Demo mode: no real milestone was added." /><DemoActionButton primary labelZh="注册项目" labelEn="Register project" messageZh="演示模式：尚未写入本地项目登记。" messageEn="Demo mode: no project was added to the local register." /></>
    : <><button className="field-action" type="button" disabled={data.projects.length === 0} aria-expanded={milestoneFormOpen} onClick={() => { setMilestoneFormOpen((value) => !value); setFormOpen(false) }}>{milestoneFormOpen ? text('收起里程碑', 'Close milestone') : text('登记里程碑', 'Add milestone')}</button><button className="field-action primary" type="button" aria-expanded={formOpen} onClick={() => { setFormOpen((value) => !value); setMilestoneFormOpen(false) }}>{formOpen ? text('收起项目', 'Close project') : text('注册项目', 'Register project')}</button></>
  return <section className="register-page"><PageHeader titleZh="项目控制点登记" titleEn="Project control-point register" descriptionZh="注册仓库、当前里程碑、健康状态与永久收据头。" descriptionEn="Registered repositories, milestones, health, and permanent receipt heads." action={action} />
    {formOpen && <form className="operation-panel" onSubmit={(event) => { event.preventDefault(); createMutation.mutate() }}><header><span className="measure-label">NEW CONTROL POINT</span><h3>{text('注册本地 Git 仓库', 'Register a local Git repository')}</h3></header><div className="operation-fields"><label>{text('显示名称', 'Display name')}<input required value={name} onChange={(event) => setName(event.target.value)} /></label><label>{text('仓库绝对路径', 'Absolute repository path')}<input required className="mono" value={rootPath} onChange={(event) => setRootPath(event.target.value)} placeholder="G:\Projects\example" /></label><label className="wide-field">{text('说明', 'Description')}<input value={description} onChange={(event) => setDescription(event.target.value)} /></label></div><footer><span>{text('路径必须存在并且是 Git 仓库根目录。', 'The path must exist and be the root of a Git repository.')}</span><button className="field-action primary" type="submit" disabled={createMutation.isPending}>{createMutation.isPending ? text('登记中…', 'Registering…') : text('确认登记', 'Register')}</button></footer></form>}
    {milestoneFormOpen && <form className="operation-panel" onSubmit={(event) => { event.preventDefault(); createMilestoneMutation.mutate() }}><header><span className="measure-label">MILESTONE DATUM</span><h3>{text('登记当前里程碑', 'Register current milestone')}</h3></header><div className="operation-fields"><label>{text('项目', 'Project')}<select required value={milestoneProjectId} onChange={(event) => setMilestoneProjectId(event.target.value)}>{data.projects.map((project) => <option key={project.id} value={project.id}>{project.name}</option>)}</select></label><label>{text('目标日期（可选）', 'Target date (optional)')}<input type="date" value={milestoneDueAt} onChange={(event) => setMilestoneDueAt(event.target.value)} /></label><label className="wide-field">{text('里程碑名称', 'Milestone name')}<input required value={milestoneName} onChange={(event) => setMilestoneName(event.target.value)} /></label><label className="wide-field">{text('结果与验收边界', 'Outcome and acceptance boundary')}<textarea value={milestoneDescription} onChange={(event) => setMilestoneDescription(event.target.value)} /></label></div><footer><span>{text('新登记项会成为项目当前活动里程碑；新任务默认归入该里程碑。', 'The new entry becomes the active milestone; new tasks default to it.')}</span><button className="field-action primary" type="submit" disabled={createMilestoneMutation.isPending}>{createMilestoneMutation.isPending ? text('登记中…', 'Registering…') : text('确认里程碑', 'Register milestone')}</button></footer></form>}
    {feedback && <p className={clsx('operation-feedback', (createMutation.isError || createMilestoneMutation.isError) && 'is-error')} role="status">{feedback}</p>}
    {data.projects.length === 0 ? <EmptyRegister titleZh="项目登记为空" titleEn="Project register is empty" detailZh="注册本地 Git 仓库后，这里会显示里程碑、健康度与收据链。" detailEn="Register a local Git repository to show milestones, health, and receipt chains here." /> : <div className="ruled-register" tabIndex={0} role="region" aria-label={text('项目控制点登记表', 'Project control-point register table')}><div className="register-head project-grid"><span>{text('控制点', 'Control point')}</span><span>{text('仓库 / 路径', 'Repository / path')}</span><span>{text('里程碑', 'Milestone')}</span><span>{text('健康', 'Health')}</span><span>{text('进度', 'Progress')}</span><span>{text('收据头', 'Receipt head')}</span></div>{data.projects.map((project, index) => <article className="register-row project-grid" key={project.id}><span className="control-cell"><i className="control-symbol" />CP-0{index + 1}</span><span><strong>{project.name}</strong><small className="mono">{project.path} · {project.branch}</small></span><span>{text(project.milestone, project.milestoneEn)}</span><span><StatusText status={project.health === 'blocked' ? 'blocked' : project.health === 'attention' ? 'review' : 'completed'} /></span><span><b className="mono">{project.progress}%</b><i className="linear-progress"><span style={{ width: `${project.progress}%` }} /></i></span><span className="mono">{project.receiptHead}</span></article>)}</div>}
  </section>
}

const columnHelper = createColumnHelper<TaskSummary>()

function WorkflowsPage() {
  const text = useText()
  const { data } = useConsoleSnapshot()
  const queryCache = useQueryClient()
  const [formOpen, setFormOpen] = useState(false)
  const [projectId, setProjectId] = useState(data.activeProjectId ?? '')
  const [title, setTitle] = useState('')
  const [description, setDescription] = useState('')
  const [workstreamName, setWorkstreamName] = useState('')
  const [dependencyIds, setDependencyIds] = useState<string[]>([])
  const [write, setWrite] = useState(false)
  const [scope, setScope] = useState('')
  const [feedback, setFeedback] = useState('')
  const [dispatchTaskId, setDispatchTaskId] = useState('')
  const [profileId, setProfileId] = useState('')
  const dispatchCandidate = data.tasks.find((task) => task.id === dispatchTaskId)
  const compatibleProfiles = dispatchCandidate?.write
    ? data.agentProfiles.filter((profile) => profile.sandboxMode === 'workspace-write')
    : data.agentProfiles
  useEffect(() => {
    if (data.projects.length > 0 && !data.projects.some((project) => project.id === projectId)) setProjectId(data.projects[0].id)
  }, [data.projects, projectId])
  useEffect(() => {
    setDependencyIds((current) => current.filter((id) =>
      data.tasks.some((task) => task.id === id && task.projectId === projectId)))
  }, [data.tasks, projectId])
  useEffect(() => {
    if (!dispatchCandidate) return
    if (!compatibleProfiles.some((profile) => profile.id === profileId)) {
      setProfileId(recommendProfile(dispatchCandidate, compatibleProfiles)?.id ?? '')
    }
  }, [compatibleProfiles, dispatchCandidate, profileId])
  const createMutation = useMutation({
    mutationFn: async () => {
      if (!data.csrfToken || !projectId) throw new Error('Live project session is unavailable')
      const scopes = scope.split(/[\n,]/).map((value) => value.trim()).filter(Boolean)
      if (write && scopes.some((value) => /^(?:[a-zA-Z]:[\\/]|[\\/])/.test(value) || value.split(/[\\/]/).includes('..'))) {
        throw new Error(text('写入范围必须是仓库内的相对路径，且不能包含 ..。', 'Write scopes must be repository-relative and cannot contain parent traversal.'))
      }
      return createTask({
        projectId,
        workstreamId: null,
        workstreamName: workstreamName.trim() || null,
        milestoneId: data.projects.find((project) => project.id === projectId)?.milestoneId ?? null,
        title,
        description,
        write,
        pathScope: write ? scopes : [],
        dependsOn: dependencyIds,
      }, data.csrfToken)
    },
    onSuccess: async () => {
      setFeedback(text('任务草稿已创建。', 'Task draft created.'))
      setTitle('')
      setDescription('')
      setWorkstreamName('')
      setDependencyIds([])
      setScope('')
      setWrite(false)
      setFormOpen(false)
      await queryCache.invalidateQueries({ queryKey: ['console-snapshot'] })
    },
    onError: (error) => setFeedback(error instanceof Error ? error.message : text('任务创建失败。', 'Task creation failed.')),
  })
  const dispatchMutation = useMutation({
    mutationFn: () => {
      if (!data.csrfToken || !dispatchCandidate || !profileId) {
        throw new Error(text('请选择一个可用角色后再派发。', 'Choose an available role before dispatching.'))
      }
      return dispatchTask(dispatchCandidate.id, profileId, data.csrfToken)
    },
    onSuccess: async () => {
      setFeedback(text('任务已派发；运行详情将持续接收事件。', 'Task dispatched; Run detail will continue receiving events.'))
      setDispatchTaskId('')
      setProfileId('')
      await queryCache.invalidateQueries({ queryKey: ['console-snapshot'] })
    },
    onError: (error) => setFeedback(error instanceof Error ? error.message : text('任务派发失败。', 'Task dispatch failed.')),
  })
  const beginDispatch = useCallback((task: TaskSummary) => {
    setFormOpen(false)
    setFeedback('')
    setDispatchTaskId(task.id)
    const profiles = task.write
      ? data.agentProfiles.filter((profile) => profile.sandboxMode === 'workspace-write')
      : data.agentProfiles
    setProfileId(recommendProfile(task, profiles)?.id ?? '')
  }, [data.agentProfiles])
  const columns = useMemo(() => [
    columnHelper.accessor('code', { header: text('测站', 'Station'), cell: (info) => <b className="mono accent-green">{info.getValue()}</b> }),
    columnHelper.accessor('title', { header: text('任务 / 范围', 'Task / scope'), cell: (info) => <span className="task-title-cell"><strong>{text(info.getValue(), info.row.original.titleEn)}</strong><small className="mono">{info.row.original.scope}</small></span> }),
    columnHelper.accessor('workstream', { header: text('工作流', 'Workstream'), cell: (info) => text(info.getValue(), info.row.original.workstreamEn) }),
    columnHelper.accessor('role', { header: text('角色', 'Role'), cell: (info) => {
      const copy = roleCopy[info.getValue()]
      return copy ? text(copy.zh, copy.en) : text('未指定', 'Unassigned')
    } }),
    columnHelper.accessor('status', { header: text('状态', 'Status'), cell: (info) => <StatusText status={info.getValue()} /> }),
    columnHelper.accessor('gate', { header: text('质量门', 'Gates'), cell: (info) => <span className="mono">{info.getValue()}</span> }),
    columnHelper.display({ id: 'actions', header: text('动作', 'Action'), cell: (info) => {
      const task = info.row.original
      const eligible = data.source === 'live' && data.runtime.available && (task.status === 'draft' || task.status === 'ready')
      const reason = data.source !== 'live'
        ? text('本地服务未连接', 'Local service is disconnected')
        : !data.runtime.available
          ? text('Codex 运行时未就绪', 'Codex runtime is not ready')
          : task.status !== 'draft' && task.status !== 'ready'
            ? text(`当前状态 ${task.status} 不可派发`, `Status ${task.status} cannot be dispatched`)
            : text('选择角色并派发', 'Choose a role and dispatch')
      return <button className="field-action compact" type="button" disabled={!eligible || dispatchMutation.isPending} title={reason} aria-expanded={dispatchTaskId === task.id} onClick={() => beginDispatch(task)}><Play size={14} />{text('派发', 'Dispatch')}</button>
    } }),
  ], [beginDispatch, data.runtime.available, data.source, dispatchMutation.isPending, dispatchTaskId, text])
  const table = useReactTable({ data: data.tasks, columns, getCoreRowModel: getCoreRowModel() })
  const dependentTask = data.tasks.find((task) => task.dependencies.length > 0)
  const downstreamTask = dependentTask
    ? data.tasks.find((task) => task.dependencies.includes(dependentTask.code))
    : undefined
  const action = data.source === 'demo'
    ? <DemoActionButton primary labelZh="新建任务" labelEn="New task" messageZh="演示模式：任务创建向导尚未启动。" messageEn="Demo mode: the task-creation flow was not started." />
    : <button className="field-action primary" type="button" disabled={data.projects.length === 0} aria-expanded={formOpen} onClick={() => setFormOpen((value) => !value)}>{formOpen ? text('收起', 'Close') : text('新建任务', 'New task')}</button>
  return <section className="register-page"><PageHeader titleZh="工作流与任务导线" titleEn="Workflow and task traverse" descriptionZh="依赖、写入范围、角色与质量门在同一登记面上保持可见。" descriptionEn="Dependencies, write scopes, roles, and gates remain visible in one register." action={action} />
    {formOpen && <form className="operation-panel" onSubmit={(event) => { event.preventDefault(); createMutation.mutate() }}><header><span className="measure-label">NEW STATION</span><h3>{text('创建任务草稿', 'Create a task draft')}</h3></header><div className="operation-fields"><label>{text('项目', 'Project')}<select required value={projectId} onChange={(event) => setProjectId(event.target.value)}>{data.projects.map((project) => <option key={project.id} value={project.id}>{project.name}</option>)}</select></label><label>{text('工作流名称（可选新建）', 'New workstream name (optional)')}<input value={workstreamName} onChange={(event) => setWorkstreamName(event.target.value)} /></label><label className="wide-field">{text('前置任务（可多选）', 'Dependencies (multiple)')}<select multiple value={dependencyIds} onChange={(event) => setDependencyIds(Array.from(event.target.selectedOptions, (option) => option.value))}>{data.tasks.filter((task) => task.projectId === projectId).map((task) => <option key={task.id} value={task.id}>{task.code} · {text(task.title, task.titleEn)}</option>)}</select></label><label className="wide-field">{text('任务标题', 'Task title')}<input required value={title} onChange={(event) => setTitle(event.target.value)} /></label><label className="wide-field">{text('说明与验收', 'Description and acceptance')}<textarea value={description} onChange={(event) => setDescription(event.target.value)} /></label><label className="check-field"><input type="checkbox" checked={write} onChange={(event) => setWrite(event.target.checked)} />{text('需要写入仓库', 'Writes to the repository')}</label>{write && <label className="wide-field">{text('允许写入范围（逗号或换行分隔）', 'Allowed write scopes (comma or newline separated)')}<textarea required className="mono" value={scope} onChange={(event) => setScope(event.target.value)} placeholder="apps/console/**" /></label>}</div><footer><span>{text('任务先保存为草稿，派发前仍会执行依赖和范围检查。', 'Tasks start as drafts; dependency and scope checks still run before dispatch.')}</span><button className="field-action primary" type="submit" disabled={createMutation.isPending || (write && !scope.trim())}>{createMutation.isPending ? text('创建中…', 'Creating…') : text('创建草稿', 'Create draft')}</button></footer></form>}
    {dispatchCandidate && <form className="operation-panel dispatch-panel" onSubmit={(event) => { event.preventDefault(); dispatchMutation.mutate() }}><header><span className="measure-label">DISPATCH STATION</span><h3>{text(`派发 ${dispatchCandidate.code}`, `Dispatch ${dispatchCandidate.code}`)}</h3><p>{text(dispatchCandidate.title, dispatchCandidate.titleEn)}</p></header><div className="operation-fields"><label>{text('Agent 角色', 'Agent role')}<select required value={profileId} onChange={(event) => setProfileId(event.target.value)}>{compatibleProfiles.map((profile) => { const copy = profileCopy(profile); return <option key={profile.id} value={profile.id}>{text(copy.zh, copy.en)} · {profile.sandboxMode === 'workspace-write' ? text('可写', 'write') : text('只读', 'read-only')}</option> })}</select></label><div className="role-brief wide-field" aria-live="polite">{(() => { const profile = compatibleProfiles.find((item) => item.id === profileId); if (!profile) return <p>{text('没有与该任务权限匹配的角色。', 'No role matches this task permission.')}</p>; const copy = profileCopy(profile); return <><strong>{text(copy.zh, copy.en)}</strong><p>{text(copy.detailZh, copy.detailEn)}</p></> })()}</div></div><footer><span>{dispatchCandidate.write ? text(`写入范围：${dispatchCandidate.scope}。将创建独立 worktree。`, `Write scope: ${dispatchCandidate.scope}. A separate worktree will be created.`) : text('只读任务不会创建 worktree，也不得修改仓库文件。', 'Read-only work will not create a worktree and must not modify repository files.')}</span><div className="panel-actions"><button className="field-action" type="button" disabled={dispatchMutation.isPending} onClick={() => setDispatchTaskId('')}>{text('取消', 'Cancel')}</button><button className="field-action primary" type="submit" disabled={dispatchMutation.isPending || !profileId}>{dispatchMutation.isPending ? text('派发中…', 'Dispatching…') : text('确认派发', 'Confirm dispatch')}</button></div></footer></form>}
    {feedback && <p className={clsx('operation-feedback', (createMutation.isError || dispatchMutation.isError) && 'is-error')} role="status">{feedback}</p>}
    {data.tasks.length === 0 ? <EmptyRegister titleZh="尚无任务导线" titleEn="No task traverse yet" detailZh="创建工作流和任务后，依赖、范围、角色与质量门会显示在这里。" detailEn="Create a workstream and tasks to show dependencies, scopes, roles, and gates here." /> : <><div className="table-frame" tabIndex={0} role="region" aria-label={text('任务登记表', 'Task register table')}><table className="data-table"><thead>{table.getHeaderGroups().map((group) => <tr key={group.id}>{group.headers.map((header) => <th key={header.id}>{flexRender(header.column.columnDef.header, header.getContext())}</th>)}</tr>)}</thead><tbody>{table.getRowModel().rows.map((row) => <tr key={row.id}>{row.getVisibleCells().map((cell) => <td key={cell.id}>{flexRender(cell.column.columnDef.cell, cell.getContext())}</td>)}</tr>)}</tbody></table></div>
    {dependentTask && <section className="dependency-strip"><div><span className="measure-label">DEPENDENCY</span><strong>{dependentTask.dependencies[0]}</strong></div><i /><div><span className="measure-label">UNLOCKS</span><strong>{dependentTask.code}</strong></div><i /><div><span className="measure-label">UNLOCKS</span><strong>{downstreamTask?.code ?? '—'}</strong></div><p>{text('写入范围存在交集时默认串行；调度器会隔离独立 worktree。', 'Overlapping write scopes serialize by default; the scheduler isolates independent worktrees.')}</p></section>}</>}
  </section>
}

function RunsPage() {
  const text = useText()
  const { data } = useConsoleSnapshot()
  const queryCache = useQueryClient()
  const [activeId, setActiveId] = useState(data.runs[0]?.id ?? '')
  const active = data.runs.find((run) => run.id === activeId) ?? data.runs[0]
  const [paused, setPaused] = useState(false)
  const [streamEvents, setStreamEvents] = useState<RunEvent[]>(data.events)
  const [streamState, setStreamState] = useState<EventStreamState>('disconnected')
  const [actionMessage, setActionMessage] = useState('')
  const [guidance, setGuidance] = useState('')
  const cancelMutation = useMutation({
    mutationFn: () => {
      if (!active || !data.csrfToken) throw new Error('Live session is unavailable')
      return cancelRun(active.id, data.csrfToken)
    },
    onSuccess: async () => {
      setActionMessage(text('已请求停止运行。', 'Run cancellation requested.'))
      await queryCache.invalidateQueries({ queryKey: ['console-snapshot'] })
    },
    onError: (error) => setActionMessage(error instanceof Error ? error.message : text('停止失败。', 'Cancellation failed.')),
  })
  const guidanceMutation = useMutation({
    mutationFn: () => {
      if (!active || !data.csrfToken) throw new Error('Live session is unavailable')
      return messageRun(active.id, guidance, data.csrfToken)
    },
    onSuccess: () => {
      setGuidance('')
      setActionMessage(text('引导已发送并写入事件日志。', 'Guidance sent and recorded in the event log.'))
    },
    onError: (error) => setActionMessage(error instanceof Error ? error.message : text('引导发送失败。', 'Guidance failed.')),
  })
  const retryMutation = useMutation({
    mutationFn: () => {
      if (!active || !data.csrfToken) throw new Error('Live session is unavailable')
      return retryRun(active.id, data.csrfToken)
    },
    onSuccess: async (result) => {
      setActionMessage(result.mode === 'resumed' ? text('失联运行已恢复。', 'Orphaned run resumed.') : text('已创建新的重试运行。', 'A new retry run was created.'))
      await queryCache.invalidateQueries({ queryKey: ['console-snapshot'] })
    },
    onError: (error) => setActionMessage(error instanceof Error ? error.message : text('重试失败。', 'Retry failed.')),
  })

  useEffect(() => {
    if (data.runs.length > 0 && !data.runs.some((run) => run.id === activeId)) setActiveId(data.runs[0].id)
  }, [activeId, data.runs])

  useEffect(() => {
    if (data.source !== 'live' || !active) {
      setStreamEvents(data.events)
      setStreamState('disconnected')
      return undefined
    }
    setStreamEvents([])
    return subscribeRunEvents(active.id, 0, (event) => {
      setStreamEvents((current) => [event, ...current.filter((item) => item.sequence !== event.sequence)].sort((a, b) => b.sequence - a.sequence))
    }, setStreamState)
  }, [active, data.events, data.source])

  const handleStop = () => {
    if (data.source === 'live') cancelMutation.mutate()
    else setPaused((value) => !value)
  }

  const openInCodex = () => {
    if (!active?.threadId) {
      setActionMessage(text('该运行还没有可打开的 Codex 线程。', 'This run does not have a Codex thread to open yet.'))
      return
    }
    window.location.href = `codex://threads/${encodeURIComponent(active.threadId)}`
    setActionMessage(text('已请求 Codex 桌面版打开该线程。', 'Asked the Codex desktop app to open this thread.'))
  }

  if (!active) {
    return <section className="register-page"><PageHeader titleZh="运行详情" titleEn="Run detail" descriptionZh="实时事件、结构化工具记录、引导提示与恢复动作，不嵌入交互式终端。" descriptionEn="Live events, structured tool records, guidance, and recovery actions without an embedded terminal." /><EmptyRegister titleZh="尚无运行" titleEn="No runs yet" detailZh="派发一个就绪任务后，运行状态和事件流会出现在这里。" detailEn="Dispatch a ready task to populate this page with run state and events." /></section>
  }

  const streamLabel = data.source === 'demo'
    ? text('演示事件', 'Demo events')
    : streamState === 'connected'
      ? text('事件流已连接', 'Event stream connected')
      : streamState === 'connecting'
        ? text('事件流连接中', 'Connecting event stream')
        : text('事件流断开，浏览器将自动重试', 'Event stream disconnected; retrying')

  const runAction = data.source === 'live' && ['failed', 'orphaned'].includes(active.status)
    ? <button className="field-action primary" disabled={retryMutation.isPending} onClick={() => retryMutation.mutate()}><Play size={15} />{active.status === 'orphaned' ? text('恢复运行', 'Resume run') : text('重试运行', 'Retry run')}</button>
    : <button className="field-action" disabled={cancelMutation.isPending || ['succeeded', 'failed', 'cancelled'].includes(active.status)} onClick={handleStop}>{paused && data.source === 'demo' ? <Play size={15} /> : <Pause size={15} />}{paused && data.source === 'demo' ? text('继续', 'Resume') : text('停止', 'Stop')}</button>

  return <section className="register-page"><PageHeader titleZh="运行详情" titleEn="Run detail" descriptionZh="实时事件、结构化工具记录、引导提示与恢复动作，不嵌入交互式终端。" descriptionEn="Live events, structured tool records, guidance, and recovery actions without an embedded terminal." action={runAction} />
    <div className="run-layout"><aside className="run-index" aria-label={text('运行列表', 'Run list')}>{data.runs.map((run) => <button type="button" key={run.id} className={clsx(activeId === run.id && 'is-active')} onClick={() => setActiveId(run.id)}><span><b className="mono">{run.taskCode}</b><StatusText kind="run" status={run.status} /></span><strong>{text(run.taskTitle, run.taskTitleEn)}</strong><small className="mono">{run.role} · {run.elapsed} · EV {run.eventSequence}</small></button>)}</aside>
      <section className="run-detail"><header className="run-datum"><div><span className="measure-label">ACTIVE RUN</span><h3>{active.taskCode} · {text(active.taskTitle, active.taskTitleEn)}</h3></div><div><span>{text('运行时', 'Runtime')}<b className="mono">{active.runtime}</b></span><span>WORKTREE<b className="mono">{active.worktree}</b></span><span>{text('耗时', 'Elapsed')}<b className="mono">{active.elapsed}</b></span></div></header>
        <div className="stream-status" role="status"><i className={clsx('live-dot', streamState === 'disconnected' && data.source === 'live' && 'is-offline')} />{streamLabel}</div>
        <div className="event-stream" tabIndex={0} role="region" aria-label={text('运行事件流', 'Run event stream')}>{streamEvents.length === 0 ? <EmptyRegister titleZh="等待运行事件" titleEn="Waiting for run events" detailZh="连接保持打开；新的状态、工具、审批与质量门事件会按序追加。" detailEn="The connection remains open; state, tool, approval, and gate events append in sequence." /> : streamEvents.map((event) => <article key={event.sequence}><time className="mono">{event.time}</time><span className={clsx('event-kind', `kind-${event.kind}`)}>{event.kind}</span><div><strong>{text(event.title, event.titleEn)}</strong><p>{text(event.detail, event.detailEn)}</p></div><b className="mono">EV {event.sequence}</b></article>)}</div>
        <footer className="run-prompt"><Code size={18} /><div><strong>{text('运行引导', 'Run guidance')}</strong>{data.source === 'live' ? <form className="guidance-form" onSubmit={(event) => { event.preventDefault(); if (guidance.trim()) guidanceMutation.mutate() }}><label className="sr-only" htmlFor="run-guidance">{text('发送给当前运行的引导', 'Guidance for the active run')}</label><input id="run-guidance" value={guidance} onChange={(event) => setGuidance(event.target.value)} placeholder={text('补充约束或纠正方向…', 'Add a constraint or correct direction…')} /><button className="field-action" type="submit" disabled={!guidance.trim() || guidanceMutation.isPending}>{text('发送', 'Send')}</button></form> : <p>{text('遵守任务声明的写入范围；质量门通过前不得提交。', 'Respect the task write scope; do not commit before quality gates pass.')}</p>}{actionMessage && <span className="action-feedback" role="status">{actionMessage}</span>}</div>{data.source === 'demo' ? <DemoActionButton labelZh="在 Codex 中打开" labelEn="Open in Codex" messageZh="演示模式：尚未连接可打开的真实 Codex 任务。" messageEn="Demo mode: no live Codex task is connected to open." /> : <button className="field-action" disabled={!active.threadId} onClick={openInCodex}>{text('在 Codex 中打开', 'Open in Codex')}</button>}</footer>
      </section>
    </div>
  </section>
}

function ApprovalsPage() {
  const text = useText()
  const { data } = useConsoleSnapshot()
  const queryCache = useQueryClient()
  const [decisions, setDecisions] = useState<Record<string, 'approved' | 'rejected'>>({})
  const [feedback, setFeedback] = useState('')
  const approvalMutation = useMutation({
    mutationFn: ({ id, decision }: { id: string; decision: 'approved' | 'rejected' }) => {
      if (!data.csrfToken) throw new Error('Live session is unavailable')
      return respondApproval(id, decision, data.csrfToken)
    },
    onSuccess: async (_response, variables) => {
      setDecisions((value) => ({ ...value, [variables.id]: variables.decision }))
      setFeedback(variables.decision === 'approved' ? text('审批已记录并发送到运行时。', 'Approval recorded and sent to the runtime.') : text('拒绝决定已记录并发送到运行时。', 'Rejection recorded and sent to the runtime.'))
      await queryCache.invalidateQueries({ queryKey: ['console-snapshot'] })
    },
    onError: (error) => setFeedback(error instanceof Error ? error.message : text('审批响应失败。', 'Approval response failed.')),
  })
  const decide = (id: string, decision: 'approved' | 'rejected') => {
    if (data.source === 'live') approvalMutation.mutate({ id, decision })
    else setDecisions((value) => ({ ...value, [id]: decision }))
  }
  return <section className="register-page"><PageHeader titleZh="审批中心" titleEn="Approval center" descriptionZh="push、merge、deploy、删除与权限提升始终由人确认，并保留决定证据。" descriptionEn="Push, merge, deploy, deletion, and elevation always require a human decision with evidence." />
    {feedback && <p className="operation-feedback" role="status">{feedback}</p>}
    {data.approvals.length === 0 ? <EmptyRegister titleZh="没有待审批事项" titleEn="No pending approvals" detailZh="需要人工确认的 push、merge、deploy、删除或权限提升会出现在这里。" detailEn="Push, merge, deploy, delete, or elevation requests that need a person appear here." /> : <div className="approval-register">{data.approvals.map((approval, index) => <article key={approval.id} className="approval-register-row"><div className="approval-number mono">AP-{String(index + 1).padStart(2, '0')}</div><div><span className="measure-label">{approval.action.toUpperCase()} · {text(approval.requestedAt, approval.requestedAtEn)}</span><h3>{text(approval.title, approval.titleEn)}</h3><p className="mono">{approval.requestedBy}<br />{approval.detail}</p></div><span className="verification-stamp">{approval.risk === 'high' ? text('高风险', 'High risk') : text('中风险', 'Medium risk')}</span><div className="approval-decision">{decisions[approval.id] ? <strong className={decisions[approval.id] === 'approved' ? 'accent-green' : 'accent-red'}>{decisions[approval.id] === 'approved' ? text('已批准', 'Approved') : text('已拒绝', 'Rejected')}</strong> : <><button className="field-action primary" disabled={approvalMutation.isPending} onClick={() => decide(approval.id, 'approved')}>{text('批准一次', 'Approve once')}</button><button className="field-action" disabled={approvalMutation.isPending} onClick={() => decide(approval.id, 'rejected')}>{text('拒绝', 'Reject')}</button></>}</div></article>)}</div>}
    <section className="protected-actions"><WarningCircle size={21} /><div><strong>{text('受保护动作不会自动放行', 'Protected actions never auto-advance')}</strong><p>{text('每个决定都绑定运行、调用者、目标、风险、时间与后续收据。', 'Each decision binds the run, requester, target, risk, timestamp, and resulting receipt.')}</p></div></section>
  </section>
}

function QualityPage() {
  const text = useText()
  const { data } = useConsoleSnapshot()
  const queryCache = useQueryClient()
  const [taskId, setTaskId] = useState(data.tasks.find((task) => task.status === 'review')?.id ?? data.tasks[0]?.id ?? '')
  const [verification, setVerification] = useState<'idle' | 'valid' | 'invalid'>('idle')
  const [feedback, setFeedback] = useState('')
  const [gateFeedback, setGateFeedback] = useState('')
  const selectedTask = data.tasks.find((task) => task.id === taskId) ?? data.tasks[0]
  const selectedRun = selectedTask
    ? data.runs.find((run) => run.taskId === selectedTask.id && run.status === 'succeeded')
      ?? data.runs.find((run) => run.taskId === selectedTask.id)
    : undefined
  const configuredGates = selectedTask
    ? data.configuredGates.filter((gate) => gate.projectId === selectedTask.projectId)
    : []
  const gateConfigurationIssue = selectedTask ? data.gateConfigurationIssues[selectedTask.projectId] : undefined
  const taskGateResults = selectedTask ? data.gates.filter((gate) => gate.taskId === selectedTask.id) : []
  useEffect(() => {
    if (data.tasks.length > 0 && !data.tasks.some((task) => task.id === taskId)) {
      setTaskId(data.tasks.find((task) => task.status === 'review')?.id ?? data.tasks[0]?.id ?? '')
    }
  }, [data.tasks, taskId])
  const verifyMutation = useMutation({
    mutationFn: () => {
      const projectId = selectedTask?.projectId ?? data.activeProjectId
      if (!projectId || !data.csrfToken) throw new Error('Live project session is unavailable')
      return verifyReceipts(projectId, data.csrfToken)
    },
    onSuccess: (result) => {
      setVerification(result.valid ? 'valid' : 'invalid')
      setFeedback(result.valid ? text('收据链验证通过。', 'Receipt chain verification passed.') : text('收据链验证失败。', 'Receipt chain verification failed.'))
    },
    onError: (error) => {
      setVerification('invalid')
      setFeedback(error instanceof Error ? error.message : text('收据链验证失败。', 'Receipt chain verification failed.'))
    },
  })
  const gateMutation = useMutation({
    mutationFn: (gate: string) => {
      if (!selectedTask || !selectedRun || !data.csrfToken) {
        throw new Error(text('任务尚无可关联的运行。', 'The task does not have a run to associate with the gate.'))
      }
      if (selectedTask.status !== 'review') {
        throw new Error(text(`任务状态为 ${selectedTask.status}，只有 review 状态可以执行质量门。`, `Task status is ${selectedTask.status}; gates run only in review.`))
      }
      return runConfiguredGate(selectedTask.id, selectedRun.id, gate, data.csrfToken)
    },
    onSuccess: async (_result, gate) => {
      setGateFeedback(text(`${gate} 已完成，结果和最终化状态已刷新。`, `${gate} completed; results and finalization state were refreshed.`))
      await queryCache.invalidateQueries({ queryKey: ['console-snapshot'] })
    },
    onError: (error) => setGateFeedback(error instanceof Error ? error.message : text('质量门执行失败。', 'Quality gate failed to run.')),
  })
  const runVerification = () => {
    if (data.source === 'live') verifyMutation.mutate()
    else {
      setVerification('valid')
      setFeedback(text('演示模式：展示已验证结果，未执行真实校验。', 'Demo mode: showing a verified result without running a live check.'))
    }
  }
  const gateRows = configuredGates.map((configured) => {
    const result = [...taskGateResults].reverse().find((gate) => gate.name === configured.name)
    return {
      ...configured,
      result,
      status: result?.status ?? 'pending' as const,
      duration: result?.duration ?? '—',
      evidence: result?.evidence ?? text('等待执行', 'Waiting to run'),
    }
  })
  const passedGates = gateRows.filter((gate) => gate.status === 'passed').length
  const head = data.metrics.receiptHead ? `${data.metrics.receiptHead.slice(0, 8)}…${data.metrics.receiptHead.slice(-4)}` : '—'
  return <section className="register-page"><PageHeader titleZh="质量门与永久收据" titleEn="Quality gates and permanent receipts" descriptionZh="命令证据按阶段登记；详细日志可清理，哈希链收据永久保留。" descriptionEn="Command evidence is recorded by stage; detailed logs may expire while hash-chain receipts remain permanent." action={<button className="field-action primary" disabled={verifyMutation.isPending || !data.activeProjectId} onClick={runVerification}>{text('验证收据链', 'Verify receipt chain')}</button>} />
    {feedback && <p className={clsx('operation-feedback', verification === 'invalid' && 'is-error')} role="status">{feedback}</p>}
    <section className="quality-controls" aria-label={text('质量门任务选择', 'Quality-gate task selection')}><label>{text('检查任务', 'Task to verify')}<select value={selectedTask?.id ?? ''} onChange={(event) => { setTaskId(event.target.value); setGateFeedback('') }}>{data.tasks.map((task) => <option key={task.id} value={task.id}>{task.code} · {text(task.title, task.titleEn)} · {task.status}</option>)}</select></label><p>{selectedTask?.status === 'review' ? text('运行成功，质量门可以逐项执行；全部通过后自动生成提交或只读收据。', 'The run succeeded. Execute gates individually; all passes create a commit or read-only receipt.') : text('选择处于 review 状态且已有成功 Run 的任务，才能执行质量门。', 'Select a task in review with a succeeded Run to execute gates.')}</p></section>
    {gateConfigurationIssue && <p className="operation-feedback is-error" role="status">{text('无法读取项目质量门配置：', 'Unable to read project gate configuration: ')} {gateConfigurationIssue}</p>}
    {gateFeedback && <p className={clsx('operation-feedback', gateMutation.isError && 'is-error')} role="status">{gateFeedback}</p>}
    <div className="quality-layout"><section className="gate-register"><header><h3>{text('当前质量门', 'Current quality gates')}</h3><span className="mono">{selectedTask?.code ?? '—'} · {passedGates} / {gateRows.length}</span></header>{gateRows.length === 0 ? <EmptyRegister titleZh="项目尚未配置质量门" titleEn="No quality gates configured" detailZh="在项目的 .ba-agent/project.toml 中配置质量门后，可在这里逐项执行。" detailEn="Configure gates in .ba-agent/project.toml to run them here." /> : gateRows.map((gate) => { const copy = gateCopy[gate.name] ?? { zh: gate.name, en: gate.name }; return <article key={gate.name}><span className={clsx('gate-icon', `gate-${gate.status}`)}>{gate.status === 'passed' ? <CheckCircle /> : gate.status === 'running' ? <Clock /> : gate.status === 'failed' ? <XCircle /> : <span />}</span><div><strong>{text(copy.zh, copy.en)}</strong><code>{gate.command}</code></div><span className="mono">{gate.duration}</span><p>{gate.evidence}</p><button className="field-action compact" type="button" disabled={data.source !== 'live' || selectedTask?.status !== 'review' || !selectedRun || gateMutation.isPending} onClick={() => gateMutation.mutate(gate.name)}>{gateMutation.isPending && gateMutation.variables === gate.name ? text('执行中…', 'Running…') : gate.result ? text('重新执行', 'Run again') : text('执行', 'Run')}</button></article> })}</section>
      <aside className="receipt-ledger"><span className="measure-label">CANONICAL JSON + SHA-256</span><h3>{text('收据链', 'Receipt chain')}</h3><div className="hash-chain" role="img" aria-label={text('四个连续收据节点', 'Four continuous receipt nodes')}><i /><span /><i /><span /><i /><span /><i /></div><dl><div><dt>{text('当前头', 'Current head')}</dt><dd className="mono">{head}</dd></div><div><dt>{text('链状态', 'Chain status')}</dt><dd className={verification === 'invalid' ? 'accent-red' : 'accent-green'}>{verification === 'valid' ? text('连续且已验证', 'Continuous and verified') : verification === 'invalid' ? text('验证失败', 'Verification failed') : text('等待验证', 'Awaiting verification')}</dd></div><div><dt>{text('永久收据', 'Permanent receipts')}</dt><dd className="mono">{data.metrics.receiptCount}</dd></div><div><dt>{text('日志保留', 'Log retention')}</dt><dd className="mono">90 DAYS</dd></div></dl><DemoActionButton labelZh="导出验证报告" labelEn="Export verification report" messageZh="当前 Alpha 尚未写出验证报告文件。" messageEn="The current Alpha does not write a verification-report file yet." /></aside></div>
  </section>
}

function SettingsPage() {
  const text = useText()
  const { language, setLanguage, theme, setTheme, visualTheme, setVisualTheme } = useUiSettings()
  const { data } = useConsoleSnapshot()
  const runtime = data.runtime
  const appServerReady = data.source === 'live' && runtime.available && runtime.appServer
  const fallbackActive = data.source === 'live' && runtime.available && !runtime.appServer && runtime.execJsonFallback
  const codexStatus = data.source !== 'live'
    ? text('等待本地服务', 'Waiting for local service')
    : !runtime.version
      ? text('未检测到 Codex CLI', 'Codex CLI not detected')
      : !runtime.authenticated
        ? text('需要登录 · codex login', 'Login required · codex login')
        : appServerReady
          ? text('首选 · 已就绪', 'Preferred · ready')
          : text('App Server 不可用', 'App Server unavailable')
  const fallbackStatus = data.source !== 'live'
    ? text('等待本地服务', 'Waiting for local service')
    : fallbackActive
      ? text('正在使用', 'Active fallback')
      : runtime.authenticated && runtime.execJsonFallback
        ? text('备用可用', 'Fallback ready')
        : runtime.execJsonFallback
          ? text('登录后可用', 'Available after login')
          : text('不可用', 'Unavailable')
  return <section className="register-page"><PageHeader titleZh="设置与适配器" titleEn="Settings and adapters" descriptionZh="完全本地、默认无遥测；运行时能力差异明确显示，不保存模型凭据。" descriptionEn="Local-only with no default telemetry; runtime capability differences stay visible and model credentials are not stored." />
    <div className="settings-layout"><fieldset><legend>{text('界面', 'Interface')}</legend><label>{text('语言', 'Language')}<select value={language} onChange={(event) => setLanguage(event.target.value as Language)}><option value="zh">中文</option><option value="en">English</option></select></label><label>{text('明暗模式', 'Appearance')}<select value={theme} onChange={(event) => setTheme(event.target.value as ThemePreference)}><option value="system">{text('跟随系统', 'System')}</option><option value="light">{text('浅色', 'Light')}</option><option value="dark">{text('深色', 'Dark')}</option></select></label><label>{text('界面主题', 'Interface theme')}<select value={visualTheme} onChange={(event) => setVisualTheme(event.target.value as VisualTheme)}><option value="fieldbook">{text('外业簿', 'Field book')}</option><option value="chartroom">{text('航图台', 'Chart room')}</option><option value="signalroom">{text('信号台', 'Signal room')}</option></select></label><label>{text('详细日志保留', 'Detailed log retention')}<select defaultValue="90"><option value="30">30 {text('天', 'days')}</option><option value="90">90 {text('天', 'days')}</option><option value="180">180 {text('天', 'days')}</option></select></label></fieldset>
      <section className="adapter-register"><header><h3>{text('运行时适配器', 'Runtime adapters')}</h3><span className="mono">CODEX {runtime.version ?? '≥ 0.130.0'}</span></header><article><span className="adapter-mark">{appServerReady ? <CheckCircle /> : <WarningCircle />}</span><div><strong>Codex App Server</strong><p>{text('实时线程、工具、审批、事件与恢复。', 'Live threads, tools, approvals, events, and recovery.')}</p>{runtime.degradedReason && <p title={runtime.degradedReason}>{!runtime.version ? text('请先安装受支持的 Codex CLI。', 'Install a supported Codex CLI first.') : !runtime.authenticated ? text('Codex CLI 尚未登录；请在终端运行 codex login。', 'Codex CLI is not signed in; run codex login in a terminal.') : text('App Server 当前不可用，已保留稳定降级路径。', 'App Server is unavailable; the stable fallback remains available.')}</p>}</div><span className={clsx('status-text', appServerReady ? 'status-completed' : 'status-review')}><i />{codexStatus}</span></article><article><span className="adapter-mark">{fallbackActive ? <CheckCircle /> : <WarningCircle />}</span><div><strong>codex exec --json</strong><p>{text('协议不兼容时自动降级；实时审批能力受限。', 'Automatic protocol fallback with limited live approvals.')}</p></div><span className={clsx('status-text', fallbackActive ? 'status-completed' : 'status-review')}><i />{fallbackStatus}</span></article><article><span className="adapter-mark"><Code /></span><div><strong>Claude compatibility</strong><p>{text('仅配置生成、兼容报告与状态导入。', 'Configuration generation, compatibility reports, and state import only.')}</p></div><span className="status-text status-ready"><i />ALPHA</span></article></section>
      <section className="privacy-register"><h3>{text('本地边界', 'Local boundary')}</h3><p>{text('服务仅绑定 127.0.0.1。不会存储 Codex 或 Claude 模型凭据，也不会自动同步到云端。', 'The service binds only to 127.0.0.1. It stores no Codex or Claude model credentials and performs no automatic cloud sync.')}</p><dl><div><dt>{text('遥测', 'Telemetry')}</dt><dd>{text('关闭', 'Off')}</dd></div><div><dt>{text('会话令牌', 'Session token')}</dt><dd>{text('每次启动随机生成', 'Random per launch')}</dd></div><div><dt>{text('数据库', 'Database')}</dt><dd>SQLite WAL · {text('完整性正常', 'Integrity healthy')}</dd></div></dl></section></div>
  </section>
}

const rootRoute = createRootRoute({ component: AppShell })
const overviewRoute = createRoute({ getParentRoute: () => rootRoute, path: '/', component: OverviewPage })
const projectsRoute = createRoute({ getParentRoute: () => rootRoute, path: '/projects', component: ProjectsPage })
const workflowsRoute = createRoute({ getParentRoute: () => rootRoute, path: '/workflows', component: WorkflowsPage })
const runsRoute = createRoute({ getParentRoute: () => rootRoute, path: '/runs', component: RunsPage })
const approvalsRoute = createRoute({ getParentRoute: () => rootRoute, path: '/approvals', component: ApprovalsPage })
const qualityRoute = createRoute({ getParentRoute: () => rootRoute, path: '/quality', component: QualityPage })
const settingsRoute = createRoute({ getParentRoute: () => rootRoute, path: '/settings', component: SettingsPage })
const routeTree = rootRoute.addChildren([overviewRoute, projectsRoute, workflowsRoute, runsRoute, approvalsRoute, qualityRoute, settingsRoute])
const router = createRouter({ routeTree, defaultPreload: 'intent' })

declare module '@tanstack/react-router' {
  interface Register { router: typeof router }
}

function initialTheme(): ThemePreference {
  const stored = window.localStorage.getItem('ba-agent-theme')
  return stored === 'light' || stored === 'dark' || stored === 'system' ? stored : 'system'
}

function initialVisualTheme(): VisualTheme {
  const stored = window.localStorage.getItem('ba-agent-visual-theme')
  return stored === 'chartroom' || stored === 'signalroom' ? stored : 'fieldbook'
}

function initialLanguage(): Language {
  return window.localStorage.getItem('ba-agent-language') === 'en' ? 'en' : 'zh'
}

export function ConsoleApp() {
  const [language, setLanguage] = useState<Language>(initialLanguage)
  const [theme, setTheme] = useState<ThemePreference>(initialTheme)
  const [visualTheme, setVisualTheme] = useState<VisualTheme>(initialVisualTheme)
  const [, startLanguageTransition] = useTransition()
  const updateLanguage = useCallback((nextLanguage: Language) => {
    startLanguageTransition(() => setLanguage(nextLanguage))
  }, [startLanguageTransition])

  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    const apply = () => {
      const resolved = theme === 'system' ? (media.matches ? 'dark' : 'light') : theme
      document.documentElement.dataset.theme = resolved
      document.documentElement.dataset.visualTheme = visualTheme
      document.documentElement.style.colorScheme = resolved
      const themeColors: Record<VisualTheme, { light: string; dark: string }> = {
        fieldbook: { light: '#f3f0e6', dark: '#131a17' },
        chartroom: { light: '#eef1ef', dark: '#0c1c26' },
        signalroom: { light: '#e9ecef', dark: '#0c1117' },
      }
      const themeColor = themeColors[visualTheme][resolved]
      document.querySelector<HTMLMetaElement>('meta[name="theme-color"]')?.setAttribute('content', themeColor)
    }
    apply()
    media.addEventListener('change', apply)
    window.localStorage.setItem('ba-agent-theme', theme)
    window.localStorage.setItem('ba-agent-visual-theme', visualTheme)
    window.localStorage.removeItem('ba-agent-palette')
    return () => media.removeEventListener('change', apply)
  }, [theme, visualTheme])

  useEffect(() => {
    document.documentElement.lang = language === 'zh' ? 'zh-CN' : 'en'
    window.localStorage.setItem('ba-agent-language', language)
  }, [language])

  const settings = useMemo(
    () => ({ language, setLanguage: updateLanguage, theme, setTheme, visualTheme, setVisualTheme }),
    [language, theme, updateLanguage, visualTheme],
  )

  return <UiSettingsContext.Provider value={settings}><QueryClientProvider client={queryClient}><RouterProvider router={router} /></QueryClientProvider></UiSettingsContext.Provider>
}
