# AGENTS.md

## 文档定位与适用范围

- 本文件适用于仓库根目录及其所有子目录。更深层目录中的 `AGENTS.md`（若新增）可补充或覆盖本文件。
- 本文件只规定实现时必须遵守的边界、验证方式和交付检查项。当前项目事实、完整调用链和行为说明统一以 [`docs/PROJECT-OVERVIEW.md`](docs/PROJECT-OVERVIEW.md) 为准，避免在两份文档中重复维护同一事实。
- `reference/` 是外部参考项目，不参与 Scourgify 构建，除非任务明确要求修改。

## 项目边界

- Windows-only、Tauri/React/Rust 架构、启动流程、IPC 分组和运行时行为统一以 [`docs/PROJECT-OVERVIEW.md`](docs/PROJECT-OVERVIEW.md) 为准；修改前先按其“最快阅读路径”定位相关模块。
- 运行时代码主要位于 `src/` 和 `src-tauri/src/`。`dist/`、`src-tauri/target/`、`node_modules/` 是生成目录；`docs/` 是说明/规划资料；`.agents/` 是技能资料。
- 前端到 Rust 的 IPC 必须经过 `src/lib/commands.ts` 的 `invokeCommand()`；组件不得直接调用 Tauri 原始 `invoke()`。IPC 命令注册和适配层位于 `src-tauri/src/cmd/`，复杂业务逻辑放在职责匹配的领域模块。

## 必须遵守的约束

### Windows 写操作与隐私

- 涉及 Quick Access 的删除、清空、Smart Clean、添加、恢复默认项或可见性修改，必须经过 `src-tauri/src/cmd/mod.rs::begin_quick_access_write(...)`；该统一入口调用 `PrivacyManager::begin_quick_access_write()` 并返回 `QuickAccessWritePermit`，写操作须持有该许可门后执行。
- 隐私模式激活时不得绕过该写入门执行受保护的写操作、规则写操作或自动清理。新增命令必须复用现有统一检查，不得在组件或领域模块中另造旁路。
- Windows API、注册表、托盘、窗口和主题逻辑保持在现有 Rust 边界内，并正确使用 Windows 条件编译；不要为跨平台环境伪造生产实现。

### 配置、IPC 与错误

- 新配置字段必须同步更新 Rust 配置模型、前端 `src/lib/config.ts`、默认值、序列化/校验和相关 UI；旧配置缺失字段必须仍可由默认值加载。
- 新 Tauri 命令必须在 `src-tauri/src/cmd/mod.rs` 注册，使用统一 `CommandResult<T>`，并返回明确 DTO 或 `ActionReceipt`。生产命令必须同步加入 release（`cfg(not(debug_assertions))`）与 debug（`cfg(debug_assertions)`）两份 handler 列表；仅 Debug Mock 命令只加入 debug 列表。
- 错误码是三层契约。新增或修改 `ErrorCode` 时，必须同时检查并更新：
  1. `src-tauri/src/error.rs` 中的枚举、wincent 错误分类和英文安全后备文案；
  2. `src/lib/commands.ts` 中按错误码映射的 `errorMessages` 五语言文案；
  3. 受影响命令、组件和测试中的错误处理。
- 对外错误保持 `CommandError { code, message, retryable, incident_id }` 契约。用户可见消息不得泄露本地路径、SQLite/IO 细节或底层错误链；完整上下文写日志并通过 `incident_id` 关联。业务分支不得解析错误字符串。

### 事件、数据与清理

- 维护两类事件契约：
  - Tauri 事件：`language-changed`、`quick-access-changed`、`auto-clean-finished` 等，由 Rust emit，并由前端监听；
  - WebView 内部 `CustomEvent`：`scourgify:refresh-dashboard`、`scourgify:refresh-history`、`scourgify:open-config-drawer`、`scourgify:auto-clean-updated` 等，由 `AppShell` 转发或消费。
- 新增后台结果事件时，必须同时检查 Rust emit、`AppShell` 转发/消费和页面刷新逻辑，并参考 [PROJECT-OVERVIEW §13](docs/PROJECT-OVERVIEW.md#13-通知与事件)。
- SQLite schema 升级必须提供迁移逻辑、迁移测试，并验证旧库可升级；数据库不可用时保留基础 Quick Access 浏览能力和 `retry_database` 原地重试路径。具体配置和数据库边界以 [PROJECT-OVERVIEW §11](docs/PROJECT-OVERVIEW.md#11-配置和数据) 及 [§12](docs/PROJECT-OVERVIEW.md#12-数据库降级) 为准。
- 清理逻辑必须保持白名单优先、`protected` 不可被规则感知清理删除、Smart Clean 仅处理 `targeted`、路径大小写不敏感去重、仅成功删除项写入历史，以及部分成功结果可见。详细语义以 [PROJECT-OVERVIEW §8](docs/PROJECT-OVERVIEW.md#8-规则与清理语义) 为准。

### 前端、国际化与 Mock

- 遵循现有 React/TypeScript 严格类型约定（包括未使用变量检查），复用 shadcn/Base UI、Tailwind、Lucide 和现有状态/事件模式；不要在页面中重复实现 IPC 错误处理。
- 用户可见文案通过 i18next 资源维护；新增或修改文案时同步检查五种语言资源：`en-US`、`zh-CN`、`zh-TW`、`fr-FR`、`ru-RU`。错误码文案仍需按上一节更新 `commands.ts` 的映射层。
- Mock 功能只在 debug handler 注册（保持 `cfg(debug_assertions)` 分流），使用独立的 Mock Quick Access 后端和数据库，不得修改真实 Windows Quick Access 数据或进入生产路径。

## 目录职责

保持以下模块边界；新增代码优先放入职责匹配的现有目录：
- `src/components/`、`src/components/ui/`：页面、交互和可复用 UI 原语。
- `src/lib/`：IPC 包装、配置类型、i18n、WebView 事件和前台通知。
- `src-tauri/src/app/`：启动、设置、窗口、托盘、主题、通知、调度和应用级事件。
- `src-tauri/src/cmd/`：Tauri IPC 薄适配层与命令注册。
- `src-tauri/src/config/`：JSON 配置加载、归一化和持久化。
- `src-tauri/src/db/`：SQLite 状态、迁移、规则仓储、历史、运行记录和统计。
- `src-tauri/src/cleanup/`：手动/自动清理用例与编排。
- `src-tauri/src/privacy/`：Quick Access 隐私锁和写入许可门。
- `src-tauri/src/quick_access/`：wincent 外部边界、操作、缓存、监控和 DTO。
- `src-tauri/src/rules/`：规则模型、导入导出和匹配器。
- `src-tauri/src/visualization/`：词云分词、范围筛选、权重和聚合。
- `src-tauri/src/mock/`：仅 Debug 的 Mock 后端和控制命令。

## 验证

仓库当前没有 CI 工作流，也没有独立的前端单测框架；前端的类型级验证包含在 `pnpm build` 执行的 `tsc` 中。按变更范围运行：

```powershell
pnpm build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

- 只有依赖清单或锁文件发生变化时才需要运行 `pnpm install`。
- 前端改动至少运行 `pnpm build`；Rust 改动至少运行 `cargo check`；涉及业务逻辑、IPC、配置、清理、隐私、迁移或事件契约时运行相关 `cargo test` 并补充测试。
- `pnpm tauri dev` 用于需要验证真实 Tauri/Windows 行为的改动，Vite 端口固定为 `1420`。
- `pnpm tauri build` 会生成 Windows MSI，属于发布级验证，按变更风险和发布需求取舍；产物位于 `src-tauri/target/release/bundle/msi/`。
- 无法运行 Windows/Tauri 验证时，明确记录未运行的命令和原因，不用跨平台猜测替代结果。

## 提交前检查

- 变更位于正确的模块边界，未误改 `reference/`、生成目录或仅供说明的规划文档。
- 新命令已注册，前端只通过 `invokeCommand()` 调用，Quick Access 写操作经过 `QuickAccessWritePermit`。
- 配置两端 schema/default、错误码三层映射和五语言文案已同步。
- 新后台事件同时覆盖 Rust emit、`AppShell` 转发/消费和页面刷新。
- 数据库迁移包含测试且旧库可升级；数据库故障仍可原地重试。
- 清理成功/失败/保护/警告统计、历史写入和隐私保护语义未被改变。
- 已运行适用的构建/测试；未运行项及原因已在交付说明中注明。
