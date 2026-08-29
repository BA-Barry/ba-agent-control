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

在项目根目录执行：

```powershell
ba-agent-control init "G:\你的仓库" --name "项目名称"
ba-agent-control doctor
ba-agent-control ui
```

`ui` 只绑定 `127.0.0.1`，并打开带一次性会话令牌的本地控制台。后续再次运行同一命令会安全地重新打开已有服务。不要复制或分享启动 URL。

## 推荐的控制台流程

1. 在“项目”确认仓库已经注册。
2. 在“工作流与任务”创建工作流、里程碑和任务。
3. 为任务填写清晰的目标、验收条件和依赖。写任务必须声明仓库相对路径范围，例如 `apps/console/**`；只读调查不要声明写入范围。
4. 将任务从 `draft` 调整到 `ready`，选择系统推荐的 Agent 角色并派发。
5. 在同一页面观察“等待启动”队列。四个槽位占满后，第五个任务会持久排队；可以上移、下移或取消。依赖未完成或写范围冲突的任务会等待，不会抢占执行。
6. 在“运行详情”查看实时事件、Agent 输出、引导、停止和重试；在“审批中心”处理 push、merge、deploy、删除或提权请求。
7. 在“质量与收据”运行项目配置的格式、lint、类型、测试、秘密扫描和 diff 审查。所有必需门禁通过后，写任务才会生成本地提交、`ba/<workstream>/<task-id>` 分支和永久收据。
8. 在“设置与适配器”调整全局共享上限和各项目上限。项目值不得高于全局值；降低上限不会中断已经运行的任务，只影响后续准入。

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
ba-agent-control task add <PROJECT_ID> "调查登录失败" --description "定位根因并给出证据"
ba-agent-control task status <TASK_ID> ready
ba-agent-control dispatch <TASK_ID> --agent-profile-id <PROFILE_ID>
ba-agent-control run list --task-id <TASK_ID>
ba-agent-control approval list
ba-agent-control receipt verify <PROJECT_ID>
```

写任务示例：

```powershell
ba-agent-control task add <PROJECT_ID> "修复登录状态机" --write --scope "crates/auth/**" --depends-on <调查任务ID>
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
