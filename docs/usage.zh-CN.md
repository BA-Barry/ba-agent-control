# BA Agent 使用说明

BA Agent 适合管理需要多个阶段、多个角色、代码隔离、人工审批和可追溯质量证据的研发工作。它不是为了替代一次性 Codex 对话；如果任务只需改一个小文件且不需要审查、依赖或证据，直接使用 Codex 通常更快。

## 适合的任务

- 跨前端、后端、数据库和文档的功能交付；
- 难以一次定位、需要 Explorer → Implementer → Test Engineer → Reviewer 的缺陷调查；
- 依赖迁移、兼容性改造、安全审查和发布准备；
- 多仓库或同仓库多个互不重叠写入范围的并行工作；
- 需要在重启后恢复，并永久保留门禁、提交和收据证据的长期任务。

八个内置角色不是八个固定并发槽。它们是可重复使用的职责模板：Coordinator 统筹，Planner 拆解，Explorer 调研，Implementer 写代码，Reviewer 审查，Test Engineer 测试，Security Reviewer 检查安全边界，Docs Reviewer 校验文档。默认全局同时运行四个 Run；角色数量可多于并发数，通常先扩展 Skill 或工作流模板，只有出现新的长期职责或权限边界时才增加角色。

## 第一次启动

先启动本地控制台：

```powershell
ba-agent-control doctor
ba-agent-control ui
```

`ui` 只绑定 `127.0.0.1`，并打开带一次性会话令牌的本地控制台。后续再次运行同一命令会安全地重新打开已有服务。不要复制或分享启动 URL。

进入“项目”后有两条安全路径：

- “导入现有仓库”：要求填写现有 Git 仓库的根目录；保留已有 `.ba-agent/project.toml`，只补齐缺少的项目目录并注册到本机数据库。
- “创建本地项目”：只接受尚不存在或完全为空的目录；初始化 `main` 分支和 `.ba-agent/` 后注册。非空目录会被拒绝，不会覆盖其中的文件。

两种操作都只发生在本机，不会创建 GitHub 仓库、配置远程、push、打 tag 或发布。CLI 等价命令如下：

```powershell
ba-agent-control project import "G:\已有仓库" --name "已有项目"
ba-agent-control project create "G:\全新项目" --name "新项目"
```

原有 `ba-agent-control init` 仍可用于导入现有仓库，并与 `project import` 共享同一套校验逻辑。

## 推荐的控制台流程

1. 在“项目”导入现有 Git 仓库或创建全新的本地仓库，并确认注册成功。
2. 在“工作流与任务”优先选择“从模板创建”。功能交付、缺陷调查、安全审查、迁移、发布准备和文档核验模板会一次性创建完整依赖链；仍可用“新建任务”登记单个例外任务。
   如果工作需要根据独立审查结果反复修复，改用“审查修复闭环”：填写明确目标、仓库相对写入范围和最多 1–3 轮修复。系统先创建实现、测试、审查三个任务，审查完成后才显示人工决定。
3. 为工作流填写仓库相对写入范围，例如 `apps/console/**`。模板会把角色、验收条件和直接依赖持久写入每个任务；只读步骤不会获得写权限。
4. 将首个可执行任务从 `draft` 调整到 `ready` 后派发。派发面板默认沿用任务已分配角色，允许人工调整为权限兼容的其他角色。
5. 在同一页面观察“等待启动”队列。四个槽位占满后，第五个任务会持久排队；可以上移、下移或取消。依赖未完成或写范围冲突的任务会等待，不会抢占执行。
6. 在“运行详情”查看实时事件、Agent 输出、引导、停止和重试；在“审批中心”处理运行时工具以及 push、merge、deploy、删除或提权请求。支持会话复用的命令与文件变更可选择“批准一次”或“本会话允许”，适用范围由当前 Codex 请求决定；MCP 确认、权限提升及受保护的 Git、部署、删除动作逐次审批。按钮依据后端返回的请求能力显示，不支持会话授权的请求会被后端拒绝。
7. 在“质量与收据”运行项目配置的格式、lint、类型、测试、秘密扫描和 diff 审查。所有必需门禁通过后，写任务才会生成本地提交、`ba/<workstream>/<task-id>` 分支和永久收据。
8. 在“设置与适配器”调整全局共享上限和各项目上限。项目值不得高于全局值；降低上限不会中断已经运行的任务，只影响后续准入。

## 整理历史任务

如果一个早期任务已经过时，但它的原 Run 并未真实完成，不要把状态伪造为 `completed`。在“工作流与任务”找到该行，选择“整理”，填写处理依据后执行以下一种操作：

- “归档为历史任务”：从默认活动登记表隐藏；
- “标记为已被替代”：同时记录同项目中的替代任务；
- “历史任务”：查看原状态和全部处理记录，填写恢复依据后可恢复到活动登记表。

归档、替代和恢复都是独立的追加记录。它们不会改写 Task 状态、依赖、Run、事件、质量门或收据。排队、运行、审查中的任务，以及仍被可见未完成下游依赖的任务，不能整理；先处理执行或依赖关系。系统也不会自动把下游依赖改接到替代任务。

```powershell
ba-agent-control task list <PROJECT_ID>
ba-agent-control task list <PROJECT_ID> --include-inactive
ba-agent-control task archive <TASK_ID> --reason "早期方案已停止使用"
ba-agent-control task supersede <TASK_ID> --replacement-task-id <NEW_TASK_ID> --reason "新任务覆盖完整恢复边界"
ba-agent-control task disposition-history <TASK_ID>
ba-agent-control task restore <TASK_ID> --reason "需要重新比较历史方案"
```

## 工作流模板与 Agent 交接

六个内置模板随二进制提供，不依赖网络。项目可以在 `.ba-agent/workflows/` 保存同名 TOML 作为显式覆盖；控制台的“复制到项目”或 CLI `materialize` 只创建新文件，已有文件会被拒绝覆盖。实例化是一个 SQLite 事务：工作流或任一任务写入失败时，整条任务链都会回滚。

```powershell
# 查看内置模板和项目覆盖诊断
ba-agent-control workstream templates <PROJECT_ID>

# 创建功能交付任务链；每个 --scope 都是仓库相对范围
ba-agent-control workstream instantiate <PROJECT_ID> feature-delivery `
  --name "登录恢复" `
  --milestone-id <MILESTONE_ID> `
  --scope "crates/auth/**" `
  --scope "apps/console/**"

# 复制内置模板到项目后再编辑；不会覆盖同名文件
ba-agent-control workstream materialize <PROJECT_ID> feature-delivery
```

上游 Agent 完成后应创建结构化交接，而不是只在聊天里留一句结论。交接目标只能是同一项目中直接依赖该上游任务的下游任务。记录保存在 `.ba-agent/knowledge/handoffs/`，包含结果、决策、证据、变更路径、命令结果、风险和下一步；下游 Run 启动或恢复时会自动注入经校验内容。

```powershell
ba-agent-control task handoff-create <SOURCE_TASK_ID> `
  --to <DOWNSTREAM_TASK_ID> `
  --outcome "已定位会话解析器的边界错误" `
  --evidence "cargo test -p auth 通过" `
  --changed-path "crates/auth/src/session.rs" `
  --risk "仍需验证旧版浏览器" `
  --next-step "先添加回归 fixture"

ba-agent-control task handoff-list <DOWNSTREAM_TASK_ID>
ba-agent-control task handoff-list <SOURCE_TASK_ID> --outgoing
```

控制台任务行的“交接”会同时显示收到和发出的记录，并在读取时核对规范 JSON、Artifact 元数据和 SHA-256 摘要。摘要用于发现尚未提交到 Git 的交接文件被替换；它不是额外的质量门。

## 受控审查—修复闭环

闭环适合“首次实现不应直接视为完成”的高风险或跨模块改动。它不是自动无限重试：每次 Reviewer 任务完成后，状态会变为 `awaiting_decision`，必须由你写明依据并选择：

- `accept`：接受当前审查结论，但仍创建一个独立确认任务；确认任务完成后闭环才是 `completed`；
- `repair`：创建新的 Implementer → Test Engineer → Reviewer 三任务阶段；达到修复轮数上限时状态变为 `exhausted`，不会继续生成任务；
- `block`：保留当前发现并停止，等待单独处理。

任何阶段任务失败、取消或阻塞都会让闭环进入 `blocked`。每个阶段都是独立 Task/Run，可继续保留质量门、交接、事件和收据；人工决定及说明也会持久保存。控制台每五秒从持久任务状态同步一次。

```powershell
# 创建最多两轮修复的闭环；目标和至少一个写入范围必填
ba-agent-control workstream loop-start <PROJECT_ID> "登录恢复闭环" `
  --objective "重启后会话、事件序号和收据保持一致" `
  --scope "crates/auth/**" `
  --scope "apps/console/**" `
  --max-repairs 2

ba-agent-control workstream loop-list <PROJECT_ID>
ba-agent-control workstream loop-show <LOOP_ID>

# Reviewer 完成后选择其一；--note 是永久决定依据，不能为空
ba-agent-control workstream loop-decide <LOOP_ID> repair --note "仍缺少锁冲突回归覆盖"
ba-agent-control workstream loop-decide <LOOP_ID> accept --note "发现均已关闭，进入独立确认"
ba-agent-control workstream loop-decide <LOOP_ID> block --note "依赖上游接口决策"
```

## 并发与队列

所有项目共享一个全局池，默认最大并发为 4。项目上限写入该仓库的 `.ba-agent/project.toml`，实际并发取项目上限与全局上限中的较小值。队列存放在全局 SQLite 中，因此关闭浏览器或重启服务不会丢失。

```powershell
# 以下队列/策略命令需要本地服务正在运行
ba-agent-control dispatch queue
ba-agent-control dispatch global 4
ba-agent-control dispatch project-limit <PROJECT_ID> 2
ba-agent-control dispatch reorder <DISPATCH_ID_1> <DISPATCH_ID_2>
ba-agent-control dispatch cancel <DISPATCH_ID>
```

调度顺序是：依赖已满足 → 未超过全局与项目上限 → 写入范围不冲突 → 按当前队列顺序启动。两个 BA Agent 控制进程同时工作时也通过 SQLite 事务竞争同一个槽位，不会重复启动或超过上限。

## CLI 最小示例

```powershell
ba-agent-control project list
ba-agent-control adapter profiles
ba-agent-control task add <PROJECT_ID> "调查登录失败" --description "定位根因并给出证据" --agent-profile-id <EXPLORER_PROFILE_ID>
ba-agent-control task status <TASK_ID> ready
ba-agent-control dispatch <TASK_ID> --agent-profile-id <PROFILE_ID>
ba-agent-control run list --task-id <TASK_ID>
ba-agent-control approval list
ba-agent-control receipt verify <PROJECT_ID>
```

写任务示例：

```powershell
ba-agent-control task add <PROJECT_ID> "修复登录状态机" --write --scope "crates/auth/**" --depends-on <调查任务ID> --agent-profile-id <IMPLEMENTER_PROFILE_ID>
```

所有命令都支持全局 `--json`，便于脚本处理。使用 `ba-agent-control <命令> --help` 查看完整参数。

## 数据在哪里

- 项目可版本控制内容：仓库内 `.ba-agent/`；
- 全局状态：操作系统应用数据目录中的 `ba-agent.db`；
- 临时 worktree、日志和本地控制描述符：同一全局数据目录；
- Codex/Claude 模型凭据：BA Agent 不保存；
- 详细运行日志：默认保留 90 天；质量收据永久保留。

## 当前边界

个人 Alpha 不提供 Claude 实时调度、模型 API 直连、团队账号、云同步或内嵌交互式终端。GitHub 远程 push、merge、发布、仓库公开化和凭据注册也不会自动执行；这些动作必须由用户单独授权。
