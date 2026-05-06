# Image Gen Kit

A Tauri desktop workbench for generating images with OpenAI-compatible image APIs.

## Features

- Generate images with `gpt-image-2` through the Image API.
- Edit images by uploading, dropping, or pasting PNG, JPEG, or WebP input images and calling the Image Edits API.
- Preserve image-edit input originals in local history so a detail record can show the source images used for that request.
- Enter a custom API key and base URL from the app UI.
- Save multiple provider profiles with aliases and switch them from the top-right provider selector.
- Configure the image request network timeout in Settings, in minutes.
- Save prompts, model, parameters, status, and local image paths in SQLite.
- Inspect a generation detail view with preserved prompt formatting, request/response payloads, and source image previews for troubleshooting.
- Receive desktop notifications when background image jobs succeed or fail.
- Keep the app running in the system tray; closing the window hides it, and the tray menu or in-app Quit button exits the app.
- Keep saved API keys out of the database. On macOS, saved keys use Keychain.
- Provider-oriented backend so additional providers, such as Google image models, can be added behind the same UI/history model.
- Browse previous generations in a gallery-style History view, then reuse, reveal, or delete a generation.

## Architecture

- Frontend: React + TypeScript + Vite.
- Desktop shell: Tauri v2.
- Backend: Rust Tauri commands own API calls, local file writes, SQLite persistence, and image-path reads.
- Image provider: `src-tauri/src/providers/openai.rs` implements the OpenAI-compatible Image API adapter.
- Persistence: `src-tauri/src/db.rs` stores provider profiles, generations, and generation outputs in SQLite.
- Local data: generated image files and the SQLite database are stored in the OS application data directory under `Image Gen Kit`.

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
- Provider type selection currently supports OpenAI-compatible providers; Google Nano Banana is listed as a TODO extension point.
- Image-edit input originals are saved for new history records only; older records created before this schema change cannot be backfilled automatically.
- Notification and tray behavior has been build-verified on Windows, but still needs manual OS-level validation for notification display and tray menu interaction.
- API key storage is Keychain-backed on macOS. Non-macOS currently falls back to a local secrets JSON file and should be replaced before cross-platform release.
- The app icon is a temporary placeholder and should be replaced before public distribution.
