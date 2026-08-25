---
name: BA Agent
description: 以“测量员外业簿”为视觉语法的本地研发控制面设计系统
colors:
  paper: "#f3f0e6"
  paper-raised: "#f8f6ef"
  paper-deep: "#e7e1d2"
  paper-ledger: "#efeade"
  ink: "#202722"
  ink-soft: "#4f5b54"
  muted: "#58635c"
  rule: "#aeb4aa"
  rule-soft: "rgb(74 88 78 / 16%)"
  traverse-green: "#2b675b"
  traverse-green-deep: "#214e45"
  traverse-green-soft: "#dce8e1"
  capacity-brass: "#805217"
  capacity-brass-soft: "#ead9b8"
  risk-red: "#a3453f"
  risk-red-soft: "#f0dcda"
  sidebar-copy: "#eef1e9"
  sidebar-muted: "#aab7ae"
  dark-paper: "#19221e"
  dark-paper-raised: "#202b26"
  dark-paper-deep: "#131a17"
  dark-paper-ledger: "#242d28"
  dark-ink: "#edf0e8"
  dark-muted: "#a9b5ad"
  dark-rule: "#526159"
  dark-traverse-green: "#77b6a6"
  dark-capacity-brass: "#d5a75a"
  dark-risk-red: "#df817a"
typography:
  page-title:
    fontFamily: '"Segoe UI", "Microsoft YaHei UI", "PingFang SC", "Noto Sans CJK SC", sans-serif'
    fontSize: "24px"
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: "-0.025em"
  shell-title:
    fontFamily: '"Segoe UI", "Microsoft YaHei UI", "PingFang SC", "Noto Sans CJK SC", sans-serif'
    fontSize: "19px"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "-0.025em"
  body:
    fontFamily: '"Segoe UI", "Microsoft YaHei UI", "PingFang SC", "Noto Sans CJK SC", sans-serif'
    fontSize: "10px"
    fontWeight: 400
    lineHeight: 1.55
  label:
    fontFamily: '"Cascadia Mono", "SFMono-Regular", Consolas, monospace'
    fontSize: "10px"
    fontWeight: 650
    lineHeight: 1.25
    letterSpacing: "0.1em"
  datum:
    fontFamily: '"Cascadia Mono", "SFMono-Regular", Consolas, monospace'
    fontSize: "9px"
    fontWeight: 650
    lineHeight: 1
rounded:
  square: "0"
  control: "2px"
  circle: "50%"
spacing:
  micro: "4px"
  xs: "8px"
  sm: "12px"
  md: "18px"
  lg: "24px"
  xl: "30px"
  measure-grid: "32px"
components:
  field-action:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0 12px"
    height: "34px"
  field-action-primary:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.paper}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0 12px"
    height: "34px"
  verification-stamp:
    backgroundColor: "{colors.risk-red-soft}"
    textColor: "{colors.risk-red}"
    typography: "{typography.datum}"
    rounded: "{rounded.square}"
    padding: "6px 8px 5px"
  icon-button:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
    size: "38px"
---

# Design System: BA Agent

## Overview

**Creative North Star: “测量员外业簿”**

BA Agent 采用已经批准并实现的 A+B 方向：A 是“控制点—导线—闭合差”的测量记录构图，B 是固定扫描顺序的事件、质量证据与审批登记。界面把仓库看作控制点、隔离 worktree 看作导线测站，把任务是否通过质量门并形成连续收据链表现为闭合是否成立；坐标、方位和测量记号是交互语法，不宣称真实地理含义。

这是高密度的 Operate 型工作面，不是营销页或聊天界面。纸张底色、墨线、表格分栏和少量状态色服务于快速核对：先判断项目健康与待办动作，再进入任务、运行、审批、质量门或收据。品牌语气保持安静、精确、可审计；示例数据必须显式标记，不能让演示动作伪装成真实写入。

批准与实现证据：

- 批准构图：`.impeccable/mocks/ba-agent-comp-approved-ab.png`
- 桌面实机复现：`.impeccable/review/hero-repro.png`
- 移动实机复现：`.impeccable/review/mobile.png`
- 构图承诺清单：`.impeccable/approved-comp-inventory.md`
- 实际组件与页面：`apps/console/src/app.tsx`
- 实际 token、布局和响应式规则：`apps/console/src/styles.css`

**Key Characteristics:**

- A+B 的“导线测站 + 固定证据登记”组合，而不是泛化仪表盘卡片墙。
- 左侧深色控制点索引、中央纸面工作区、右侧行动账页、底部收据链形成桌面骨架。
- 1px 石墨分隔为常态；2px 只强调主导线、登记边界、选中 datum 或关键闭合边界。
- 状态色稀少且语义稳定：绿表示运行/完成/连续，黄铜表示容量/排队/审查，红表示风险/阻塞/失败。
- 中文为默认界面，英文是完整等价资源；代码、哈希、时间、测量值和运行状态才使用等宽字。

### 信息架构与七页继承

七个路由共享同一个 `AppShell`、设置入口、语言/主题状态、顶部 datum 和导航顺序：

1. **总览 `/`**：唯一使用完整三段式构图的页面。中央上部为连续导线测站，下部为 append-only 事件与质量证据登记；右栏处理审批与闭合差；底部显示收据链和主行动。
2. **项目 `/projects`**：项目控制点登记，显示仓库、路径、里程碑、健康、进度和收据头。
3. **工作流与任务 `/workflows`**：语义表格登记任务与范围，另用依赖条表达解锁次序与写入范围规则。
4. **运行详情 `/runs`**：运行索引、活动运行 datum、事件流和当前引导 prompt；不嵌入交互式终端。
5. **审批中心 `/approvals`**：按编号的审批登记与成对决定动作，并保留受保护动作提示。
6. **质量与收据 `/quality`**：质量门登记与收据账页并置，展示命令证据、链状态和永久收据。
7. **设置与适配器 `/settings`**：使用原生字段与登记分区表达界面偏好、运行时能力和本地边界。

除总览外，页面继承 `register-page → page-header → page-specific register` 的语法：页首明确“是什么、为何看”，右侧只放示例标记与当前页主动作；主体选择登记表、索引+详情、审批条、质量账页或 fieldset，不复制总览导线图作为装饰。

## Colors

浅色主题像长期使用但整洁的外业纸张；深色主题保留同一信息层级，把纸、墨、规则线和状态色成组反转，而不是简单套黑色滤镜。frontmatter 中的颜色是实际 CSS 变量值，状态和表面必须通过语义变量使用。

### Primary

- **导线绿 `traverse-green`**：主导线、控制点、运行/完成/成功、链连续和选中信息的操作色；悬停底色只混入约 5%，避免铺满页面。
- **深导线绿 `traverse-green-deep`**：主按钮悬停和成功反馈文字；不得替代正文墨色。

### Secondary

- **容量黄铜 `capacity-brass`**：并发容量、排队、启动、审查和等待审批；也是全局可见焦点轮廓。
- **柔黄铜 `capacity-brass-soft`**：文本选择等低强度背景，不作为大面积品牌色。

### Tertiary

- **风险红 `risk-red`**：高/中风险印章、阻塞、失败、失联和受保护动作边界。
- **柔风险红 `risk-red-soft`**：低强度风险底色；红色表达需要人工注意，不表达普通强调。

### Neutral

- **外业纸 `paper`**：浅色主工作面与测站针底色。
- **抬升纸 `paper-raised`**：测站标签、菜单、字段等稍高表面。
- **深纸 `paper-deep`**：收据页脚、依赖条、运行 prompt 等结构性分区。
- **账页纸 `paper-ledger`**：总览右侧审批与闭合差账页。
- **石墨墨 `ink`**：正文、主要边界和主行动底色。
- **次级墨 `muted`**：辅助说明、表头和元数据；不能用于关键动作。
- **规则线 `rule` / `rule-soft`**：分别用于主要分隔与表内细分。
- **暗色组**：`dark-paper*`、`dark-ink`、`dark-muted`、`dark-rule` 与三种 `dark-*` 状态色一一替换浅色角色；侧栏固定为深墨场，不随浅色主题漂白。

**The State-Semantics Rule.** 运行中/完成/成功用绿；排队/启动/审查/等待审批用黄铜；阻塞/失败/失联用红。不要根据页面自行重新解释状态色。

**The One-Accent-at-a-Time Rule.** 同一局部只允许一个主状态色承担注意力；正文、边界和结构仍以墨色/规则线为主。

## Typography

**Display/Body Font:** Segoe UI，依次回退至 Microsoft YaHei UI、PingFang SC、Noto Sans CJK SC、sans-serif。  
**Datum/Mono Font:** Cascadia Mono，依次回退至 SFMono-Regular、Consolas、monospace。

字体组合像工作簿里的说明文字与仪器读数：中文工作字体承担叙事和操作，等宽字体只承担可以核验、对齐或逐项比较的数据。正文与控件大多处于 8–12px 的密集区间，但关键标题和闭合值明确放大。

### Hierarchy

- **页面标题**：24px / 700 / `-0.025em`；在 ≤600px 收至 20px。
- **壳层标题**：19px / 700 / 1；在 ≤900px 收至 17px。
- **区块标题**：13–18px；总览导线 18px、行动账页 17px、登记区通常 13–15px。
- **正文/说明**：9–12px，说明段落行高约 1.4–1.6；页首说明最大 72ch，本地边界说明最大 76ch。
- **测量标签**：等宽 10px / 650 / 1.25，字距 0.1em，全大写；用于 TRAVERSE、DATUM、DEPENDENCY、序列和量测说明。
- **表格与 datum**：8–10px 等宽、启用 tabular numerals；用于任务码、路径、hash、时间、事件序号、门计数、比例与运行时状态。
- **闭合值**：28px 等宽，用作当前总览唯一的巨大数值焦点。

**The Semantic Mono Rule.** 不把整页或普通中文正文改成等宽字体。只有代码、哈希、时间、测量值、任务/事件编号和机器状态使用 mono。

**The Bilingual-Equivalence Rule.** 所有核心文本和动作通过同一组件的中英成对资源呈现；默认 `zh-CN`，英文切换为 `en`，并同步更新根元素 `lang`。翻译可以长度不同，但不得删减风险、状态、单位或下一步动作。

## Layout

桌面壳层使用 228px 固定侧栏与自适应工作区，侧栏粘在视口并满高；顶部 datum 高 76px。总览在其下最小高 720px，采用“主工作面 + 302px 行动账页”两列和 82px 收据页脚；主工作面再按“导线场 + 232px 事件登记”上下分区。总览的三列拓扑不可被普通卡片栅格替换。

登记页面以 30px 页面内边距、84px 页首和 24px 主体起距建立节奏。具体内容维持其数据结构：项目登记使用明确列宽，任务表最小宽 920px，运行页是 260px 索引 + 详情，质量页是自适应质量门 + 310px 收据账页。空间单位主要落在 4/8/12/18/24/30/32px；更小的 5/7/9/10/13/14/15/16/20/22/25/27/28px 是现有高密度组件的局部微调，不应扩张成新的通用规模。

### 测量网格边界

24px 网格只用于总览导线场，32px 网格只用于具有“测量/登记”语义的登记页面。网格是量测坐标场，不是品牌纹理；设置表单、菜单、弹层、侧栏、页脚或未来非登记型内容不得为了“有风格”而添加 decorative grid，也不得添加栅格噪点和仿旧位图纹理。

### 响应式

- **≤1180px**：隐藏顶部里程碑 datum，右侧行动账页缩至 270px，并收紧项目/审批列宽；仍保留桌面拓扑。
- **≤900px**：侧栏改为顶部深色区域，导航横向滚动；隐藏控制点索引和侧栏脚注。总览改为纵向文档流，导线画布保持至少 820px 并允许水平滚动，事件表保持至少 760px；行动账页落到导线/登记之后，收据页脚换行。此时显示固定的移动 action rail。
- **≤600px**：移除次要品牌副标、topbar context 与北向标；页首/运行 datum/审批/质量门/设置表单进一步单列重排。数据不通过缩小到不可读来“适配”，而是换行、分区或水平滚动。
- 页面最小支持宽度为 320px；固定 action rail 尊重 `env(safe-area-inset-bottom)`，总览为其预留底部空间。

### 新增页面的继承顺序

1. 先把页面加入既有七项导航语法，沿用 Phosphor 线性图标、双语标签和右侧短码/计数。
2. 默认复用 `AppShell` 与登记页骨架；只有页面确实表达依赖、隔离测站与闭合证据时才使用导线几何。
3. 从页面任务选择已实现的登记模式：ruled register、data table、index-detail、approval register、gate + receipt ledger 或 fieldset/register 组合。
4. 页首只保留标题、简短解释、DemoMark（若为示例）和一个主动作；不创建独立 hero 或 KPI 卡片带。
5. 在 1180/900/600px 三个边界验证布局，并检查中英文长文本、320px 宽度、水平滚动和 action rail 遮挡。

## Elevation & Depth

系统默认扁平，深度主要由纸色分层、1px/2px 规则线和固定分区建立，而不是卡片阴影。唯一的通用阴影 token 是浅色 `0 10px 28px rgb(32 39 34 / 10%)`、深色 `0 12px 32px rgb(0 0 0 / 24%)`；它只用于浮层菜单/tooltip 和移动 action rail。测站标签使用更轻的单向下投影 `0 8px 20px rgb(32 39 34 / 8%)`，且只有顶部 1px 墨线，不再加四边框。

**The Flat-by-Default Rule.** 登记表、审批条、质量门、侧栏分区和普通页面容器都靠纸色与规则线分层；不要给每个区块加阴影或制造浮动卡片墙。

## Shapes

形状以方正、可量测为主：普通控件与选择框只有 2px 微圆角，登记表和结构容器保持直角，不建立 8/12/16px 卡片圆角系统。状态点和测站针使用圆形；项目控制点与收据节点由 45° 旋转方形形成菱形。风险/示例印章保持 1px 同色边框并轻微旋转 -1°，表现人工批注但不能影响可读性。

主结构线为 1px；只有主导线 3px，分支导线 1.5px 虚线，关键登记边界/闭合分隔/选中 datum 为 2px。选中测站从 18px 圆针放大为 26px，并用黄铜环和墨色填充形成唯一主测站；阻塞测站只把针与中心点变红，不改变几何含义。

**The No-Rounded-Card Rule.** 不为新登记区、审批条或数据表添加大圆角。方形纸面和连续规则线是该系统的基本轮廓。

## Components

### 壳层与导航

- **AppShell**：228px 深色侧栏、76px topbar 和可滚动主工作区；浅深主题下侧栏都保持 `#202722` 的稳定索引场。
- **主导航**：桌面每项最小高 43px，20px 图标列 + 文本 + 右侧短码；活动项使用 2px 淡黄铜左线与 8% 白色底。≤900px 改为横向滚动，并把活动线移到底部。
- **设置入口**：38×38px、1px 墨线、2px 圆角；hover 反转为墨底纸字。Radix tooltip/dropdown 使用真实菜单语义、键盘导航和同一焦点系统。

### 状态文本

`StatusText` 是“小圆点 + 双语状态”的统一模式。running/completed/succeeded 为绿；review/waiting approval/starting/queued 为黄铜；blocked/failed/orphaned 为红；其余状态保留 muted。颜色旁始终有可读文字，不能只靠颜色传达状态。

### 导线测站

导线几何使用响应式 SVG，语义测站使用真实按钮叠放其上；SVG 本身 `aria-hidden`。测站标签包含任务码、双语标题、状态和 gate 证据；按下后通过 `aria-pressed` 更新右栏“当前测站”，右栏以 `aria-live="polite"` 宣告变化。几何必须对应真实依赖/隔离关系；不得用随机折线填空。

### 登记表与事件流

项目、任务、事件、质量门和适配器都采用固定扫描顺序、细规则线、紧凑行高和语义 HTML。事件与证据登记明确标注 `APPEND-ONLY`，最新事件向下扫描；宽表在窄屏水平滚动并保留最小宽度。hover 只用约 5% 导线绿混色，选中运行索引才允许整块绿底反白。

### 审批条与受保护动作

审批条必须同时显示请求动作、调用者/目标、风险印章、请求时间和决定动作。批准与拒绝成对出现；push、merge、deploy、删除和提权不能因视觉上的“主按钮”而自动放行。受保护动作区用 1px 风险红边界和 4% 红色底强调规则，不使用模态恐吓或大面积红底。

### 收据链

总览底部是“节点—连线—节点 + 连续状态 + HEAD hash + 主动作”；质量页使用四个菱形节点和连线，并列出链状态、永久收据数和日志保留期。链的视觉连续必须对应真实可验证状态；断链不能继续显示绿色连续线。

### DemoMark

`DemoMark` 复用 `verification-stamp`：风险红文字与边框、低强度红底、等宽 9px、轻微 -1° 旋转。所有 synthetic demonstration 页面或数据必须显示“示例数据 / Demo data”；该标记不是一般标签、状态 chip 或营销贴纸。

### DemoActionButton

`DemoActionButton` 复用 field action，点击后只显示带 CheckCircle 的 `role="status"` 反馈，明确“演示模式”及没有发生的真实写入/导出/连接；不得静默修改持久状态。反馈使用深导线绿、最大宽 250px。若以后接入真实动作，应替换演示反馈并实现真实 loading/success/error/disabled 状态，不保留误导性的“已执行”文案。

### Field action

- **普通**：最小高 34px、水平 padding 12px、1px 墨线、2px 圆角、透明底；hover 只混入约 7% 墨色。
- **Primary**：墨色底、纸色字；hover 改为深导线绿。一个行动区只设一个 primary。
- **Focus**：全局 3px 黄铜 outline，offset 3px；不得以去除 outline 换取整洁。
- **Disabled**：透明度 0.46、`not-allowed` 光标并阻止动作；不能只改变外观却仍响应点击。

### 移动 action rail（mobile action rail）

≤900px 才显示：左右各 12px 固定，底部为 `max(12px, safe-area)`，最小高 58px，1px 墨线，深纸 96% 混合背景和唯一通用阴影。左侧显示当前测站，右侧是“打开运行”主动作；长测站码/标题用省略，不允许 rail 承载多个次级动作或遮挡正文。

### 主题、语言与交互状态

- 主题支持 system/light/dark 并持久化；system 跟随 `prefers-color-scheme`，同时设置 `color-scheme` 和浏览器 `theme-color`。
- 语言支持中文/English 并持久化；中文为默认，所有核心流程在两个语言下保持等价。
- 主要可操作项使用 button、link、select 等原生/语义控件；装饰 SVG 和图标设置 `aria-hidden`，链图有可读 `aria-label`。
- skip link 在聚焦时进入视口并指向可聚焦的 `main`；键盘顺序跟随视觉扫描顺序。
- 只允许状态驱动的 hover、菜单和轻微反馈；不使用环境漂浮、路线流光或无休止动画。`prefers-reduced-motion: reduce` 下全局将过渡/动画压至 0.01ms，关闭平滑滚动并限制一次迭代。

## Do's and Don'ts

### Do:

- **Do** 从用户要完成的操作和可核验证据选择登记模式，先保持扫描顺序，再决定视觉表达。
- **Do** 为新页面继承 AppShell、双语标题、主题 token、1px 规则线、方正控件和已有响应式断点。
- **Do** 把真正的依赖、worktree 隔离、gate 结果、审批风险和收据状态映射到导线、登记与链条。
- **Do** 在示例数据和演示动作旁持续使用 DemoMark / DemoActionButton 的诚实边界。
- **Do** 同时验证浅色、深色、中文、英文、键盘、320px 窄屏、≤900px action rail 和 reduced motion。
- **Do** 当数据很宽时保留可读字号并提供水平滚动；当层级改变时重排，不把桌面页面整体缩小。

### Don't:

- **Don't** 在测量/登记语境之外使用 decorative grid；不得把网格铺进设置字段、菜单、侧栏、页脚或普通说明页。
- **Don't** 把 approved comp PNG 作为生产背景或截图组件；生产界面必须继续由语义 HTML/CSS、响应式 SVG 和真实数据构成。
- **Don't** 把任何新页面做成圆角 KPI 卡片墙、玻璃拟态、渐变 hero、聊天气泡或嵌入式终端。
- **Don't** 随机添加导线、坐标或测量术语。只有真实依赖/隔离/闭合证据允许使用这套几何语法。
- **Don't** 只靠颜色、图标或动画表达状态；必须保留文字、结构和可读的 focus/aria 语义。
- **Don't** 让演示按钮、受保护动作或 disabled 控件产生未说明的真实副作用。
- **Don't** 为了塞下英文或移动布局而删除风险、单位、范围、时间、证据或下一步动作。
