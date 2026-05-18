# Image Gen Kit

A Tauri desktop workbench for generating images with OpenAI-compatible image APIs.

## Features

- Generate images with `gpt-image-2` through the Image API.
- Edit images by uploading, dropping, or pasting PNG, JPEG, or WebP input images and calling the Image Edits API.
- Preserve image-edit input originals in local history so a detail record can show the source images used for that request.
- Reuse identical image-edit input files by content hash instead of copying the same source image for every request.
- Enter a custom API key and base URL from the app UI.
- Save multiple provider profiles with aliases and switch them from the top-right provider selector.
- Use xAI Grok Imagine as a provider type with the default `https://api.x.ai/v1` base URL and `grok-imagine-image-quality` model.
- Configure the image request network timeout in Settings, in minutes.
- Switch OpenAI-compatible image responses between `url` and `b64_json` when comparing provider behavior.
- Save prompts, model, parameters, status, input-image references, and local output image paths in SQLite.
- Inspect a generation detail view with preserved prompt formatting, request/response payloads, and source image previews for troubleshooting.
- Receive desktop notifications when background image jobs succeed or fail.
- Keep the app running in the system tray; closing the window hides it, and the tray menu or in-app Quit button exits the app.
- Keep saved API keys out of the database. On macOS, saved keys use Keychain.
- Provider-oriented backend so additional providers, such as Google image models, can be added behind the same UI/history model.
- Browse previous generations in a gallery-style History view, then reuse, retry, reveal, or delete a generation.
- Retry a generation from History or the Generate inspector while preserving the saved request parameters and reference images.
- Keep the app chrome fixed while scrolling: the left rail and top provider/action bar stay visible.
- Keep the Generate page embedded History panel lightweight with the latest 10 records and internal scrolling.
- Browse the full History gallery with pagination, total counts, direct page jumps, provider labels, and lazy-loaded thumbnails.

## Architecture

- Frontend: React + TypeScript + Vite.
- Desktop shell: Tauri v2.
- Backend: Rust Tauri commands own API calls, local file writes, SQLite persistence, and image-path reads.
- Image provider: `src-tauri/src/providers/openai.rs` implements the OpenAI-compatible adapter and xAI Grok Imagine request mapping.
- Persistence: `src-tauri/src/db.rs` stores provider profiles, generations, generation outputs, and input-image references in SQLite.
- Local data: the SQLite database and image files are stored in the OS application data directory under `Image Gen Kit`.
- Image storage: generated outputs are stored under `images/outputs`, while reusable edit inputs are stored under `images/inputs` by SHA-256 content hash.

The frontend never calls OpenAI directly. This avoids browser CORS issues and keeps API key handling inside the Tauri backend.

## Development

```bash
npm install
npm run tauri:dev
```

## Build and Verification

```bash
npm run build
cd src-tauri && cargo test
cd src-tauri && cargo check
npm run tauri -- build
```

Current macOS packaging target is the `.app` bundle. Full DMG packaging was intentionally not enabled as the default target because Tauri's generated DMG script failed in this environment even though the release binary and `.app` bundle built successfully.

On Windows, `src-tauri/tauri.windows.conf.json` overrides the bundle target to NSIS:

```bash
npm run tauri -- build --ci --no-sign
```

The Windows installer is written to `src-tauri/target/release/bundle/nsis/Image Gen Kit_0.1.0_x64-setup.exe`. Current installer builds are unsigned and should be signed before public distribution.

## Current Limits

- `gpt-image-2` text-to-image generation and image edit requests are implemented. Mask-based local edits are not implemented yet.
- Google/Nano Banana support is planned as a future provider adapter.
- Provider type selection currently supports OpenAI-compatible providers and xAI Grok Imagine. Google Nano Banana is listed as a TODO extension point.
- xAI Grok Imagine uses provider-specific controls: `aspect_ratio`, `resolution` (`1k` or `2k`), and `response_format: "b64_json"`.
- xAI single-image edit sends a JSON `image` data-URI reference and follows the input image aspect ratio.
- xAI multi-image edit sends JSON `images` data-URI references, supports up to 3 input images, and can override `aspect_ratio`.
- Retry replays saved request JSON and input-image references. API keys are not stored in history, so retry requires the provider's saved key or the active provider key field.
- Image-edit input originals are saved for new history records only; older records created before this schema change cannot be backfilled automatically.
- Reusable input images are intentionally not deleted when a single generation is deleted. Add reference-counting or garbage collection before cleaning `images/inputs`.
- Existing pre-dedup input images are not migrated automatically into `images/inputs`.
- Multi-output generations are listed in detail records. The preview modal can still be improved with explicit output switching.
- Full History now paginates instead of rendering the whole library, but gallery thumbnails still use original image payloads lazily rather than generated low-resolution thumbnail files.
- Notification and tray behavior has been build-verified on Windows, but still needs manual OS-level validation for notification display and tray menu interaction.
- API key storage is Keychain-backed on macOS. Non-macOS currently falls back to a local secrets JSON file and should be replaced before cross-platform release.
- The app icon is a temporary placeholder and should be replaced before public distribution.
