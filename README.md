# Image Gen Kit

A Tauri desktop workbench for generating images with OpenAI-compatible image APIs.

## Features

- Generate images with `gpt-image-2` through the Image API.
- Enter a custom API key and base URL from the app UI.
- Save prompts, model, parameters, status, and local image paths in SQLite.
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

## Current Limits

- `gpt-image-2` text-to-image generation is implemented; image editing and multi-image workflows are not implemented yet.
- Google/Nano Banana support is planned as a future provider adapter.
- API key storage is Keychain-backed on macOS. Non-macOS currently falls back to a local secrets JSON file and should be replaced before cross-platform release.
- The app icon is a temporary placeholder and should be replaced before public distribution.
