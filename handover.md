# Image Gen Kit 交接记录

## 2026-05-07 输入图去重、多输出预览与 Generate 布局修复

### 1. 本次会话目标 / 当前阶段目标

本次目标是修正 Image Edit 和 History 在真实使用中的几个可扩展性问题：多张输出图在 Detail/Preview 中应可见，图生图输入原图不应每次重复复制，输入图和输出图不应混在同一个目录，Generate 页左侧导航和右侧 History 应在生成时保持稳定。当前改动是阶段性长期方案：输入图按内容哈希复用，输出图仍按生成月份归档；后续如需清理孤儿输入图，需要单独做引用计数或 GC。

### 2. 当前仓库状态

- 当前分支：`main`。
- 当前远端：`origin git@github.com:loo-y/image-gen-kit.git`。
- 当前最新提交：`089d6447838544ac515b2b4ad568d1685a938e11`，已推送到 `origin/main`。
- 本次主要改动文件：`src-tauri/src/app_paths.rs`、`src-tauri/src/providers/openai.rs`、`src-tauri/src/db.rs`、`src-tauri/src/types.rs`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、`src/App.tsx`、`src/styles.css`。
- 当前 Windows NSIS 安装包路径：`src-tauri/target/release/bundle/nsis/Image Gen Kit_0.1.0_x64-setup.exe`。
- 当前 Windows NSIS 安装包 SHA256：`680CBBC541C3A96C0A72971A04769A581ABA848F25DBDEDFF9F44D03C5F97C0D`。
- 本文档更新发生在代码提交之后，作为单独文档同步提交进入远端历史。

### 3. 今天实际遇到的问题

1. History Detail 之前把 input 和 output 放在同一块区域，多输出场景下容易遗漏输出图展示；用户明确指出 output 多张时 Preview/Detail 需要考虑。
2. Image Edit 输入图和输出图都写到同一个 `images/<month>` 目录，目录语义混乱，不利于排查和后续清理。
3. 同一张输入图被多次用于图生图时，每次都会复制一个新的 input 文件，磁盘浪费明显。
4. 旧的删除逻辑会在删除某一条 generation 时同时删除输入图；输入图改为共享引用后，这种删除方式会误删其他历史仍在引用的原图。
5. 左侧侧边栏会随页面滚动；Generate 页点击 `Generate image` 后，旁边 History 面板高度会被同一 Grid 行里的 Composer/Inspector 内容撑高。
6. Generate 页 History 需要最多显示 10 条；超过可视高度时，应在 History 模块内部滚动，而不是撑开整个应用。
7. 左上角品牌标识仍是 `IG`，用户要求改成 `IGK`。

### 4. 原因判断与结论

- 输入图重复来自文件命名策略：之前按 generation id 和 input index 生成文件名，所以相同内容无法复用。
- 输入/输出目录混乱来自 `generation_image_dir` 同时服务 input 和 output；应拆成 `images/inputs` 和 `images/outputs/<bucket>`。
- 输入图共享后，删除 generation 时不能再直接删除 input 文件；当前正确做法是只删除 output 文件和 DB generation 记录，保留共享输入图。
- History 高度问题来自 CSS Grid 默认 `align-items: stretch` 和 history 面板只有 `max-height` 没有固定 `height`；生成后其他列变高会影响同一行视觉高度。
- 当前未做自动 GC 是有意约束：没有引用计数前，不应猜测某个 input 文件是否可删。

### 5. 这次已经落地的修复

- `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`
  - 新增 `sha2`，用于对输入图内容计算 SHA-256。

- `src-tauri/src/app_paths.rs`
  - 新增 `input_images_dir()`，输入图统一保存到 `images/inputs`。
  - 新增 `output_images_dir()`，输出图统一保存到 `images/outputs`。
  - 将输出图归档函数改为 `generation_output_image_dir()`，继续按月份 bucket 存放输出。

- `src-tauri/src/providers/openai.rs`
  - 保存输入图时使用内容 SHA-256 作为文件名，路径形如 `images/inputs/<hash>.<ext>`。
  - 如果相同 hash 文件已经存在，不再重复写入。
  - 输出图保存改走 `generation_output_image_dir()`，不再和输入图混目录。

- `src-tauri/src/db.rs`、`src-tauri/src/types.rs`
  - `generation_input_images` 增加 `content_hash` 字段，并用 `ensure_column` 兼容已有数据库。
  - `GenerationInputImage` 类型增加 `content_hash`。
  - 删除 generation 时只返回输出图路径用于删除，不再删除共享 input 文件。

- `src/App.tsx`
  - 左上角品牌从 `IG` 改为 `IGK`。
  - Generate 页 History 只渲染最新 10 条。
  - Detail 中 Metadata 和 Input Images 同行展示，Output Images 单独展示，支持多输出记录。

- `src/styles.css`
  - 左侧 rail 改为固定定位，宽度随响应式断点保持 112px/94px。
  - Generate 三列 Grid 改为 `align-items: start`，避免 History 被其他列撑高。
  - History 面板固定 `height: calc(100vh - 124px)`，内部列表 `flex: 1` 并独立纵向滚动。
  - Input Images 区域固定在可用宽度内，图片多时内部横向滚动。

### 6. 已验证结果

本阶段实际验证通过：

- `cmd /c npm run build`：TypeScript 和 Vite production build 通过。
- `cargo fmt --check`：Rust 格式检查通过。
- `cargo check`：Rust 类型检查通过。
- `cargo test`：11 个 Rust 单元测试通过。
- `cmd /c npm run tauri -- build --ci --no-sign`：Windows release binary 和 NSIS 安装包生成成功。
- 最后一次安装包 SHA256：`680CBBC541C3A96C0A72971A04769A581ABA848F25DBDEDFF9F44D03C5F97C0D`。

未验证：

- 未安装最新 NSIS 包做人工 UI 回归。
- 未用真实 OpenAI API key 验证多输出图、输入图复用和删除历史后的文件保留行为。
- 未验证长期使用后 `images/inputs` 中孤儿文件的清理策略。

### 7. 踩过的坑 / 已否定方案 / 关键约束

- 不要继续按 generation id 复制 input 文件；这会让同一张源图被多次保存。
- 不要在删除某条 generation 时直接删除 input 图；输入图现在是共享资源，除非先实现引用计数或 GC。
- 不要把 input 和 output 重新放回同一个目录；排障时会难以区分源图和生成结果。
- 不要只给 History 加 `max-height`；CSS Grid stretch 仍可能让视觉高度被其他列影响，必须固定 `height` 并设置 Grid 对齐。
- 当前 `content_hash` 是按图片字节算的，文件名不同但内容相同会复用；内容经过重编码后 hash 会不同，这是预期边界。

### 8. 接手后如何继续

1. 先看 `src-tauri/src/providers/openai.rs` 的 `persist_input_images()`，确认输入图 hash 命名和不重复写入的行为。
2. 再看 `src-tauri/src/db.rs` 的 `delete_generation()`，确认删除历史只清理输出图。
3. 看 `src-tauri/src/app_paths.rs`，确认输入输出目录拆分规则。
4. 看 `src/App.tsx` 的 `HistoryView` 和 `GenerationDetailModal`，确认 Generate 页 History 限制 10 条，以及 Detail 多输出展示。
5. 看 `src/styles.css` 的 `.rail`、`.contentGrid`、`.historyPane`、`.historyList`，确认固定侧边栏和 History 内部滚动。
6. 本地验证建议先跑 `cmd /c npm run build`、`cargo test`、`cargo check`。
7. 如果继续发包，跑 `cmd /c npm run tauri -- build --ci --no-sign` 并记录新的 installer SHA256。

### 9. 当前仍存在的问题 / 边界

- `images/inputs` 暂无孤儿文件清理；这是为了避免误删共享输入图。
- 旧历史记录中已复制到旧路径的 input 文件不会自动迁移到 `images/inputs`。
- History Preview 对多输出图的交互仍可继续增强，例如在 modal 内切换多张 output；当前重点是 Detail 能完整展示多输出。
- 安装包仍未签名。
- 仍未做真实 API 端到端回归。

### 10. 最终想实现的产品目标

最终目标仍是普通用户可安装的桌面图片生成和编辑工具：输入图、输出图、请求参数、响应和错误信息都应可追溯；相同输入图应可复用而不浪费磁盘；History 在长任务、多图、多历史记录场景下应保持稳定可读。当前版本已经把存储模型向这个目标推进了一步，但还需要真实 API 回归、安装包签名和输入图 GC。

### 11. 后续 TODO

1. 实现输入图引用计数或安全 GC。
   - 目的：清理 `images/inputs` 中不再被任何 `generation_input_images` 引用的文件，避免长期使用后磁盘无限增长。

2. 做真实 API 多图回归。
   - 目的：验证多输出 response、Detail 输出展示、输入图 hash 复用、删除历史后的文件保留行为都符合预期。

3. 增强 Preview 多输出切换。
   - 目的：当一次请求有多张 output 时，用户不只在 Detail 看列表，也能在 Preview modal 中切换查看。

4. 为已有旧数据设计迁移策略。
   - 目的：如果用户已经有旧版图生图历史，评估是否需要把旧 input 文件迁移到 `images/inputs` 并补齐 `content_hash`。

5. 安装最新 NSIS 包做人工 UI 回归。
   - 目的：确认固定侧边栏、Generate 页 History 固定高度、内部滚动、Detail 多输出和输入图横向滚动在真实 WebView 中表现正确。

## 2026-05-06 系统通知与托盘更新

- 已新增系统通知：`src-tauri/src/commands.rs` 在后台 `start_generation` 任务结束后，根据 `openai::run_existing_job` 的成功/失败结果调用 `tauri-plugin-notification` 发送系统通知；通知内容包含模型、供应商、输出数量或失败摘要。
- 已新增系统托盘：`src-tauri/src/lib.rs` 启用 Tauri `tray-icon` feature，创建托盘图标和右键菜单，菜单包含 `Show Image Gen Kit` 和 `Quit`。
- 窗口关闭行为已改为默认隐藏到托盘：`WindowEvent::CloseRequested` 中调用 `api.prevent_close()` 并隐藏窗口；真正退出需要托盘 `Quit` 或应用内 `Quit` 按钮。
- 应用内已增加 `Tray` 和 `Quit` 按钮：`src/App.tsx` 调用新的 Tauri commands `minimize_to_tray` 和 `quit_app`；`src/styles.css` 增加对应按钮样式。
- 新增依赖：`src-tauri/Cargo.toml` 增加 `tauri-plugin-notification = "2"`，`tauri` feature 增加 `tray-icon`；`Cargo.lock` 因插件引入了桌面通知和托盘相关依赖。
- 已验证：`cmd /c npm run build`、`cargo fmt --check`、`cargo check`、`cargo test`、`cmd /c npm run tauri -- build --ci --no-sign` 均通过。
- 未验证：没有人工安装最新 NSIS 包并实际点击托盘菜单；没有用真实 OpenAI API key 等待后台任务完成来确认 Windows/macOS 通知弹出。

## 2026-05-06 收尾提交交接

### 1. 本次会话目标 / 当前阶段目标

本阶段目标是把 Image Gen Kit 从基础生成 MVP 推进到可在 Windows 上安装验证的 OpenAI-compatible 图片工作台。范围包括：Windows NSIS 安装包、Image Edit 输入图、尺寸配置补齐、历史记录排障信息、多供应商 profile、网络超时设置、Preview/Detail 弹窗体验，以及本次提交前的交接文档同步。当前方案是可验证的阶段方案，仍不是正式公开分发版本。

### 2. 当前仓库状态

- 当前分支：`main`。
- 当前远端：`origin git@github.com:loo-y/image-gen-kit.git`。
- 本次准备一起提交的主要文件：`src/App.tsx`、`src/styles.css`、`src-tauri/src/providers/openai.rs`、`src-tauri/src/db.rs`、`src-tauri/src/types.rs`、`src-tauri/src/commands.rs`、`src-tauri/src/sqlite.rs`、`src-tauri/src/main.rs`、`src-tauri/tauri.windows.conf.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、`README.md`、`handover.md` 和 Tauri 生成的跨平台 icon 文件。
- Windows 最新安装包路径：`src-tauri/target/release/bundle/nsis/Image Gen Kit_0.1.0_x64-setup.exe`。
- 最新安装包 SHA256：`DAAD09F6C2C363BAE0CC137A1B612D111A0BDD58CEFE2797567207A524930B36`。
- 本次交接文档更新会随同本次代码变更提交并推送到 `origin/main`；`src-tauri/target`、`dist`、`node_modules` 和 `.omx` 仍由 `.gitignore` 排除。

### 3. 今天实际遇到的问题

1. Windows 安装后启动会弹出 cmd 终端。触发条件是 release binary 没有声明 Windows GUI subsystem，会影响普通用户安装后的桌面体验。
2. Windows 打包时 Rust 链接缺少 `sqlite3.lib`。触发条件是本地没有系统 SQLite import library，导致 Tauri release build 无法稳定完成。
3. 生成尺寸选项不完整，无法直接选择用户要求的 square/landscape/portrait/2K/4K/auto 组合。
4. 只支持文生图，不支持传入图片做 image edit；前端也缺少文件拖拽和剪贴板图片读取。
5. 多张图片并发生成时 History 排序会随轮询完成顺序变化，导致列表视觉上跳动。
6. 长请求仍可能超时，但超时时间不是用户可配置项。
7. History 卡片直接展示错误信息，列表噪音大；Detail 里也缺少完整 request/response，排障不方便。
8. Preview 弹窗在小视窗下提示词和图片会重叠；Detail 中 prompt 没保留原始换行格式。
9. 图生图历史只保存输出图，没有保存输入原图，无法回看这次 edit 基于哪些源图。
10. 右上角 `Save settings` 和 provider 下拉视觉粗糙，按钮文字会折行。

### 4. 原因判断与结论

- Windows cmd 弹窗是 Tauri/Rust release 程序 subsystem 配置问题，不是安装器问题；正确修复点是 `src-tauri/src/main.rs`。
- Windows SQLite 链接问题来自依赖系统 `sqlite3.lib`，更稳的分发方案是改用 `libsqlite3-sys` bundled SQLite，而不是要求用户或 CI 额外安装 SQLite SDK。
- History 排序跳动来自前端轮询 upsert 时把更新项重新插到数组顶部；稳定方案是前后端统一按 `created_at DESC, id DESC` 排序。
- Detail request/response 应存文本 JSON，输入原图不应塞进 JSON 或 SQLite blob；当前结论是图片继续落盘，DB 只保存路径和元数据。
- Google/Nano Banana 目前只保留 provider type TODO，不应混入 `openai.rs`，后续要新增独立 provider adapter。

### 5. 这次已经落地的修复

- `src-tauri/src/main.rs`：增加 release-only `windows_subsystem = "windows"`，修复 Windows 安装后启动弹 cmd 的问题。
- `src-tauri/src/sqlite.rs`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`：改为使用 bundled `libsqlite3-sys`，避免 Windows 缺少 `sqlite3.lib`。
- `src-tauri/tauri.windows.conf.json` 和 `src-tauri/icons/*`：Windows bundle target 固定为 NSIS，并补齐 Tauri 打包所需图标。
- `src/App.tsx`、`src/styles.css`：补齐尺寸选项；新增 Image Edit 模式；支持上传、拖拽、剪贴板图片；优化 Settings、provider switcher、按钮和下拉框样式；Debug 默认勾选。
- `src-tauri/src/providers/openai.rs`：新增 `/images/edits` multipart 请求；支持 OpenAI-compatible response 解析；记录 redacted request 和 response；增加可配置网络超时。
- `src-tauri/src/db.rs`、`src-tauri/src/types.rs`：增加 `response_json`、`network_timeout_minutes` 和 `generation_input_images`；历史查询使用稳定排序；删除历史时同步删除输出图和输入图。
- `src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`：新增读取拖拽输入图 data URL 的 Tauri command，并把图生图输入原图保存/读取链路串起来。
- `README.md`：补充 Windows NSIS 打包命令、安装包输出位置、Image Edit 输入图历史保存和当前限制。
- `handover.md`：记录 Windows 打包、Image Edit、多供应商、超时、历史详情、Preview/Detail 和 Debug 默认勾选的交接信息。

### 6. 已验证结果

本阶段实际验证通过：

- `cmd /c npm run build`：TypeScript 和 Vite production build 通过。
- `cargo fmt --check`：Rust 格式检查通过。
- `cargo check`：Rust 类型检查通过。
- `cargo test`：7 个 Rust 单元测试通过。
- `cmd /c npm run tauri -- build --ci --no-sign`：Windows release binary 和 NSIS 安装包生成成功。
- 最新 installer：`Image Gen Kit_0.1.0_x64-setup.exe`，大小约 3.4 MB，SHA256 为 `DAAD09F6C2C363BAE0CC137A1B612D111A0BDD58CEFE2797567207A524930B36`。

未验证：未使用真实 OpenAI API key 做线上文生图/图生图端到端调用；未安装运行最新 NSIS 包做人工 UI 回归；安装包未签名。

### 7. 踩过的坑 / 已否定方案 / 关键约束

- 不要重新引入系统 SQLite 链接依赖；Windows 上会回到 `sqlite3.lib` 缺失问题。
- 不要把图生图输入原图放进 `params_json` 或 `response_json`；这会让 SQLite 历史记录膨胀，也会拖慢列表查询。
- 不要把完成中的 generation 按轮询返回顺序插到 History 顶部；必须保持稳定排序。
- 不要把错误详情直接显示在 History 卡片；当前设计是卡片只显示摘要，Detail 才展示错误/request/response。
- `--no-sign` 只适合本地测试包，不是公开分发方案。

### 8. 接手后如何继续

1. 先读 `README.md` 的 Features、Build and Verification、Current Limits，确认当前能力和未完成项。
2. 再读本文件顶部两个 `2026-05-06` 章节，理解 Windows 打包、Image Edit 和 History schema 的改动边界。
3. 本地恢复后先跑 `cmd /c npm run build`、`cargo test`、`cargo check`。
4. 如果要验证 Windows 包，运行 `cmd /c npm run tauri -- build --ci --no-sign`，检查 NSIS 输出路径。
5. 手动验证优先顺序：保存 provider/API key、文生图、图生图上传/拖拽/粘贴、History Detail 的 request/response 和输入原图、Preview 小视窗滚动。
6. 如果 History 原图不显示，优先查 `generation_input_images.path` 是否在 app images 目录内，以及 `read_image_data_url` 是否拒绝了路径。

### 9. 当前仍存在的问题 / 边界

- 安装包未签名，Windows SmartScreen/安全提示没有处理。
- 没有真实 API key 端到端验证线上 OpenAI 图片生成和编辑。
- 旧的图生图历史记录无法自动补回输入原图；只有 schema 更新后的新记录会保存。
- Mask-based image edit 没有实现。
- Google/Nano Banana 只是禁用 TODO provider type，后端 adapter 未实现。
- Windows/Linux API key 仍是本地 JSON fallback，不适合最终分发。
- 当前 app icon 是临时占位。

### 10. 最终想实现的产品目标

最终目标仍是一个可交付普通用户安装的桌面图片生成工具：用户可以配置多个 provider，稳定生成/编辑图片，浏览可追溯的历史记录，查看完整排障信息，并获得签名安装包和正式图标。当前版本已经打通 Windows NSIS 安装包和 Image Edit 主链路，但仍需要真实 API 验证、安全存储和正式分发处理。

### 11. 后续 TODO

1. 用真实 OpenAI API key 验证文生图和图生图端到端链路，确认 request 参数、response 解析、图片落盘、History Detail 全部可用。
2. 安装最新 NSIS 包做人工 UI 回归，重点看启动无 cmd、Preview 小视窗、Detail prompt 换行、输入原图预览。
3. 给 Windows 安装包接入代码签名，避免公开分发时出现不必要的安全提示。
4. 替换正式 app icon，删除临时占位视觉。
5. 为 Google/Nano Banana 新增独立 provider adapter，不要污染 OpenAI-compatible adapter。
6. 替换 Windows/Linux API key 本地 JSON fallback，接入系统级安全存储。
7. 增加自动化集成测试或 API mock，覆盖成功响应、HTTP error、图生图 multipart、输入图保存和历史删除。

## 2026-05-06 Windows 打包与 Image Edit 更新

- Windows NSIS 安装包已生成成功，路径为 `src-tauri/target/release/bundle/nsis/Image Gen Kit_0.1.0_x64-setup.exe`。
- Windows 启动弹出 cmd 的原因是 release binary 缺少 `windows_subsystem = "windows"`；已在 `src-tauri/src/main.rs` 增加 release-only GUI 子系统声明。
- Windows 打包缺少 `sqlite3.lib` 的问题已修复：`src-tauri/src/sqlite.rs` 改为复用 `libsqlite3-sys` 绑定，`Cargo.toml` 使用 `bundled` SQLite。
- Windows bundle 默认目标已通过 `src-tauri/tauri.windows.conf.json` 覆盖为 `nsis`，不会影响 macOS 默认 `.app` 配置。
- Generate 页尺寸选项已补齐：`1024x1024`、`1536x1024`、`1024x1536`、`2048x2048`、`2048x1152`、`3840x2160`、`2160x3840`、`auto`，默认使用 `auto`。
- 已新增 Image Edit MVP：前端支持上传、拖拽、粘贴 PNG/JPEG/WebP 输入图，后端根据输入图自动走 OpenAI-compatible `POST /images/edits` multipart 请求；mask 局部编辑 UI 尚未实现。
- History 排序已改为前后端稳定排序：DB 使用 `created_at DESC, id DESC`，前端轮询更新不再把完成项强行插到顶部。
- Settings 已增加网络超时配置，单位分钟，范围 1-120，保存到 provider profile 并传入本次请求。
- History 卡片不再直接显示错误详情；新增 Detail 弹窗，展开后查看错误、元信息、本次 request 和 response，便于排障。
- Detail 弹窗的 prompt 已改为保留原始换行格式；Preview 弹窗在小视窗下改为滚动布局，避免图片和提示词重叠。
- 图生图输入原图已随历史保存到 app images 目录，DB 使用 `generation_input_images` 记录路径和元数据，Detail 中按需加载原图预览；删除历史时会一并删除输入原图文件。
- Generate 页 Debug 模式默认勾选，便于默认保留请求/响应调试文件。
- 右上角 provider 下拉和 `Save settings` 按钮已重做样式，避免按钮文字折行。
- Settings 已支持多供应商 profile：可新建供应商、填写供应商别名、选择 provider type；目前只启用 OpenAI-compatible，Google Nano Banana 作为禁用 TODO 类型保留。右上角 provider selector 可直接切换已保存供应商，label 已移动到下拉框左侧。
- 已验证：`npm run build`、`cargo fmt --check`、`cargo check`、`cargo test`，以及 Windows `npm run tauri -- build --ci --no-sign`。

## 2026-05-06 会话总结

### 1. 本次会话目标 / 当前阶段目标

本次目标是从空目录实现一个 Tauri 桌面工具 MVP，用于通过 OpenAI-compatible Image API 调用 `gpt-image-2` 生成图片。用户要求支持手动输入 `apiKey` 和 `baseUrl`，并考虑未来接入 Google Nano Banana 等模型；同时要求使用本地数据库把生成图片、提示词、模型和参数保存起来，便于后续查看。

交互方向参考了 `nexu-io/open-design` 的工具型工作台风格，但没有下载完整仓库，只参考 README 和相关 skill 思路。当前阶段是 MVP：主链路、历史图库、本地存储和 macOS `.app` 打包已打通，但还不是最终分发版本。

### 2. 当前仓库状态

当前目录原本不是 git 仓库，本次会话新建了完整项目骨架。核心文件包括：

- `src/App.tsx`：React 主界面，包含 Generate、History、Settings 三个视图。
- `src/styles.css`：应用布局和图库/工作台样式。
- `src-tauri/src/commands.rs`：Tauri command 层，负责前端调用入口、生成请求调度、图片读取、删除和 reveal。
- `src-tauri/src/providers/openai.rs`：OpenAI-compatible 图片生成 adapter，负责请求构造、响应解析、base64 图片落盘和参数校验。
- `src-tauri/src/db.rs` 与 `src-tauri/src/sqlite.rs`：本地 SQLite 初始化、CRUD 和轻量 SQLite FFI wrapper。
- `src-tauri/src/secrets.rs`：API key 本地保存逻辑，macOS 使用 Keychain。
- `src-tauri/tauri.conf.json`：Tauri 应用配置，默认 bundle target 为 `["app"]`。
- `README.md`：已补充功能、架构、运行验证命令和当前限制。

本次文档更新后，应把仓库初始化、提交并推送到 `git@github.com:loo-y/image-gen-kit.git`。

### 3. 今天实际遇到的问题

1. 初始目录为空，且不是 git 仓库。需要从零创建 Tauri + React + Rust 项目结构。
2. npm 默认 registry 指向公司 artifactory，并因为代理 `it-hkproxy.cc.ctripcorp.com` 解析失败导致依赖查询失败。最终使用 `npm install --registry https://registry.npmjs.org` 安装前端依赖。
3. Cargo 初次拉取依赖时也受代理解析失败影响；在获得网络权限后重新运行，依赖下载和 Rust 编译通过。
4. Tauri `generate_context!()` 默认需要 `src-tauri/icons/icon.png`，缺少图标时 Rust 编译报错。当前放入了一个临时 icon，让构建链路通过。
5. `npm run tauri -- build` 使用 `targets: "all"` 时，release binary 和 `.app` 已成功产出，但 DMG 阶段在 Tauri 生成的 `bundle_dmg.sh` 失败。为避免默认构建失败，将 `src-tauri/tauri.conf.json` 的 bundle target 改为 `["app"]`。
6. 用户检查 UI 后指出 `Generate` 和 `History` tab 切换后都展示 prompt、history、result 三个模块，只是位置变化；这不符合 History 应该是图库列表的预期。随后将 History 重构为独立图库页。

### 4. 原因判断与结论

- API 调用放在 Rust backend 中是当前正确方向：避免浏览器 CORS，同时避免在前端直接暴露长期保存的 API key。
- SQLite 放在本地应用数据目录，图片以文件形式落盘，DB 保存路径和元数据；这比把图片二进制全部塞进 DB 更适合 MVP 和后续图库浏览。
- Provider 抽象目前以 `provider_type` 和 OpenAI adapter 起步，后续 Google/Nano Banana 应新增 provider adapter，不应把 Google 逻辑混进 `openai.rs`。
- History tab 原实现是布局复用过度导致的产品边界问题；用户反馈后已确认 History 应该是图库视图，而不是生成页的变体。
- DMG 失败发生在 macOS 打包脚本层，不影响 release binary 和 `.app` 的生成。当前先默认打 `.app`，DMG 作为后续分发专项处理。

### 5. 这次已经落地的修复

- `package.json` / `vite.config.ts` / `tsconfig.json` / `index.html`
  - 新建 React + TypeScript + Vite 项目基础。
  - 增加 `dev`、`build`、`tauri:dev`、`tauri:build` 等脚本。

- `src/App.tsx`
  - 实现 Generate 工作台：prompt、model、size、quality、format、compression、moderation、生成按钮。
  - 实现 Settings：baseUrl、默认 model、API key、是否保存 API key。
  - 实现 History 独立图库页：搜索栏、图片网格、缩略图、prompt/model/size/time 元信息、`Use` / `Reveal` / `Delete` 操作。
  - `Use` 会将历史记录回填到 Generate 页面继续编辑或复用。

- `src/styles.css`
  - 实现克制的桌面应用布局：左侧 rail、顶部状态区、生成三栏工作台、图库网格、结果 inspector。
  - 修复 History tab 不是图库的问题，避免 tab 切换后只是三个模块换位置。

- `src-tauri/src/commands.rs`
  - 暴露 `init_app`、`save_provider_profile`、`generate_image`、`list_generations`、`read_image_data_url`、`reveal_image`、`delete_generation` 等 Tauri commands。
  - 生成任务通过 Rust backend 执行，不从浏览器端直接请求 OpenAI。

- `src-tauri/src/providers/openai.rs`
  - 实现 OpenAI-compatible `/images/generations` 请求。
  - 默认支持 `gpt-image-2` 参数：`prompt`、`size`、`quality`、`output_format`、`output_compression`、`moderation`。
  - 解析 `b64_json`，落盘为本地图片文件，并写入 output 记录。
  - 增加尺寸、质量、格式、压缩参数的基础校验和单元测试。

- `src-tauri/src/db.rs` / `src-tauri/src/sqlite.rs`
  - 建表：`provider_profiles`、`generations`、`generation_outputs`。
  - SQLite 使用系统 `sqlite3` FFI wrapper，避免在前端引入浏览器数据库。

- `src-tauri/src/secrets.rs`
  - macOS 保存 API key 到 Keychain。
  - 非 macOS 目前是 MVP fallback：本地 `secrets.json`，后续跨平台分发前必须替换。

- `src-tauri/tauri.conf.json`
  - 配置 Tauri v2 窗口、CSP、bundle target。
  - 默认只打 `.app`，避免 DMG 脚本失败阻断常规构建。

### 6. 已验证结果

本次实际执行并通过：

- `npm run build`
  - TypeScript 编译通过。
  - Vite production build 通过。

- `cd src-tauri && cargo fmt --check`
  - Rust 格式检查通过。

- `cd src-tauri && cargo test`
  - 3 个 OpenAI provider 单元测试通过：
    - 接受合法 size。
    - 拒绝非法 size。
    - 正确拼接 image generation endpoint。

- `cd src-tauri && cargo check`
  - Rust backend 类型检查通过。

- `npm run tauri -- info`
  - Tauri 环境和配置检查通过。

- `npm run tauri -- build`
  - release binary 成功生成。
  - macOS `.app` bundle 成功生成：
    - `src-tauri/target/release/bundle/macos/Image Gen Kit.app`

未真实验证：

- 没有使用真实 OpenAI API key 发起一次线上图片生成。
- 没有验证 Windows/Linux 打包。
- 没有验证 Google/Nano Banana provider。

### 7. 踩过的坑 / 已否定方案 / 关键约束

- 不要把 OpenAI 请求放到前端直接调用：会遇到 CORS 和 API key 暴露问题。当前已放在 Rust backend。
- 不要把 History 设计成 Generate 页的布局变体：用户已明确指出 History 应是图库列表。
- 不要默认启用 `targets: "all"`：当前环境下 DMG 脚本失败，`.app` 可以正常生成。
- 不要把 `.omx`、`node_modules`、`dist`、`src-tauri/target`、`src-tauri/gen` 提交到 GitHub；`.gitignore` 已覆盖这些路径。
- 当前 icon 是临时占位，不应作为正式产品图标发布。
- 非 macOS API key fallback 不是最终安全方案。

### 8. 接手后如何继续

建议接手顺序：

1. 先看 `README.md`，确认运行命令和当前限制。
2. 再看 `src/App.tsx`，理解 Generate / History / Settings 的 UI 状态和 command 调用。
3. 再看 `src-tauri/src/commands.rs`，理解前端和后端之间的接口。
4. 再看 `src-tauri/src/providers/openai.rs`，理解 provider adapter 的边界。
5. 本地先跑：
   - `npm install`
   - `npm run build`
   - `cd src-tauri && cargo test`
   - `npm run tauri -- build`
6. 打开 `src-tauri/target/release/bundle/macos/Image Gen Kit.app`，优先手动验证：
   - Settings 输入 baseUrl/API key。
   - Generate 发起一次真实生成。
   - History 是否展示图库卡片。
   - `Use` 是否能回填 prompt 和参数。
   - `Reveal` 是否能定位本地图片。
   - `Delete` 是否删除 DB 记录和本地图片。

### 9. 当前仍存在的问题 / 边界

- 真实 OpenAI 生成链路未用线上 API key 验证。
- 目前只实现 text-to-image，没有 image edit、多图输入、批量生成 UI。
- SQLite FFI wrapper 是 MVP 方案，可用但不如 `rusqlite`/`sqlx` 生态成熟；如果后续数据库逻辑变复杂，建议评估替换。
- 当前 History 缩略图直接读取原图为 data URL，历史很多时需要做缩略图缓存或分页加载优化。
- macOS Keychain 已实现；非 macOS secrets fallback 不适合正式分发。
- DMG packaging 仍未解决；当前默认只打 `.app`。
- icon 是临时占位。

### 10. 最终想实现的产品目标

最终目标是一个可分发给普通用户使用的桌面图片生成工具：

- 用户可以配置不同 provider 的 API key/baseUrl/model。
- 用户可以稳定生成、浏览、复用、删除历史图片。
- 历史记录保留 prompt、model、provider、参数和本地图片路径。
- 后续接入 Google/Nano Banana 等模型时，不破坏现有历史库和 UI 主流程。
- 最终应提供可靠安装包和正式图标，而不是只依赖开发态运行。

### 11. 后续 TODO

1. 用真实 OpenAI API key 端到端验证 `gpt-image-2` 生成。
   - 目标：确认请求参数、`b64_json` 响应解析、图片落盘、SQLite 记录、History 图库展示全部真实可用。

2. 为 provider 增加更明确的 adapter trait。
   - 目标：接入 Google/Nano Banana 时只新增 adapter，不改动主 UI 和历史 DB 结构。

3. 替换正式 app icon。
   - 目标：避免发布时使用临时占位图标。

4. 解决 DMG packaging。
   - 目标：恢复 macOS 安装包分发能力；当前 `.app` 构建可用，但不是完整安装体验。

5. 增加缩略图缓存或分页。
   - 目标：History 图片多时减少一次性读取原图 data URL 的内存和启动成本。

6. 加强跨平台密钥存储。
   - 目标：Windows/Linux 不再使用本地明文-ish JSON fallback。

7. 增加真实 API mock 集成测试。
   - 目标：不依赖真实 OpenAI key，也能验证生成成功、API 失败、无 `b64_json`、图片文件写入和 DB 状态更新。
