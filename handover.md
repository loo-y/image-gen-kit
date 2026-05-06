# Image Gen Kit 交接记录

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
