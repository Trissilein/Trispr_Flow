# Video Generation — Phase 1a Verification

**Status:** Code complete, **render pipeline smoke-tested end-to-end without Trispr UI**. Remaining verification is limited to UI drag-and-drop and the Tauri command round-trip.

## What's been auto-verified (no human needed)

- `cargo check --lib` and `cargo build --lib` both pass cleanly.
- `npx tsc --noEmit` on the frontend: zero errors in new code (one pre-existing error in `workflow-agent-console.ts` unrelated to this feature).
- hyperframes 0.4.9 installed at `D:\GIT\Trispr_Flow\src-tauri\bin\hyperframes\` (142 packages, no vulnerabilities).
- Chrome Headless Shell 131 downloaded to `%USERPROFILE%\.cache\hyperframes\` (auto-managed by hyperframes).
- **End-to-end render proof:** the `slideshow.html.tmpl` template was manually substituted with sample items, the pipeline produced a valid 1920×1080 @ 30 fps H.264 MP4 in 8.3 seconds (788 KB). Proof artifact: `C:\Users\trist\AppData\Local\Temp\hf-smoke\out.mp4`.
- `%LOCALAPPDATA%\Trispr Flow\settings.json` pre-populated with `video_generation_settings.enabled = true` and `hyperframes_cwd` pointing to the bundled install.

## Important correction from initial plan

hyperframes' CLI does **not** take a standalone HTML file. It takes a **project directory** containing `hyperframes.json`, `index.html`, and `meta.json`. The Rust code was corrected accordingly (`compose_hyperframes_project` instead of `compose_static_template`). Also, the `--ffmpeg-path` flag does not exist — hyperframes manages its own FFmpeg internally.

## Preconditions

**All of these are already satisfied on this machine** — listed for reproducibility.

1. **Node 22+ on PATH.** This machine uses nvm-for-Windows; switch with:
   ```powershell
   nvm use 24.11.1
   node --version    # v24.11.1
   ```
   Important: `npm run dev` inherits whichever Node is current in the shell you launch it from. If you switch Node *after* starting a dev session, restart the session.

2. **hyperframes installed** at `D:\GIT\Trispr_Flow\src-tauri\bin\hyperframes\`. Re-install with:
   ```powershell
   cd D:\GIT\Trispr_Flow\src-tauri\bin\hyperframes
   npm install hyperframes
   ```

3. **Chrome Headless Shell downloaded** (one-time, ~101 MB):
   ```powershell
   cd D:\GIT\Trispr_Flow\src-tauri\bin\hyperframes
   node node_modules\hyperframes\dist\cli.js browser ensure
   ```

4. **Environment variable overrides** (optional — settings.json already points to the right install):
   ```powershell
   $env:TRISPR_NODE_BINARY = "C:\path\to\node.exe"
   $env:TRISPR_HYPERFRAMES_CWD = "C:\path\to\hyperframes\install"
   ```

## Enabling the module

The `VideoGenerationSettings.enabled` flag defaults to `false`. Before first use:

```powershell
# Find the Trispr Flow settings file:
notepad $env:LOCALAPPDATA\Trispr Flow\settings.json
```

Edit the `video_generation_settings` block:

```json
"video_generation_settings": {
  "enabled": true,
  "output_dir": "",
  "default_resolution": "1920x1080",
  "default_fps": 30,
  "default_style": "slideshow",
  "tts_provider": "none",
  "node_binary_path": "",
  "hyperframes_cwd": "",
  "max_upload_mb": 500
}
```

## Steps

1. **Start the app in dev mode:**
   ```powershell
   cd D:\GIT\Trispr_Flow
   npm run dev
   ```

2. **Confirm the "Video" tab is present** in the main nav bar (between "Voice Output" and "Assistant Debug"). Click it.

3. **Test file drop:** Drag-and-drop a mix of files onto the drop zone:
   - Markdown (`.md`), plain text (`.txt`)
   - JSON (`.json`, `.yaml`)
   - An image (`.png`, `.jpg`)
   - An audio file (`.mp3`)
   - A video file (`.mp4`)

   Each file should appear in the Queue list with:
   - Position index
   - Filename
   - Kind badge (green = content, purple = asset, blue = hybrid)
   - First 120 chars of extracted text (content files) or original path (assets)
   - A `×` remove button

4. **Confirm error handling for unsupported extensions:** Drop a `.exe` or `.zip` → the progress area shows a clear error message, the file is not added to the queue.

5. **Confirm size limit:** Set `max_upload_mb` to `1` in settings.json, drop a file > 1 MB → progress shows "exceeds limit".

6. **Pick files via button:** Click "Pick Files..." → native file dialog opens, multi-select works.

7. **Generate a video:**
   - Queue at least one content-kind source (e.g. a `.md` file).
   - Optionally add one or two image assets.
   - Set Style = Slideshow, Resolution = 1920x1080, FPS = 30, TTS = Off.
   - Optional brief: "Short overview, dark background."
   - Click "Generate Video".

8. **Verify progress streams:** The progress area should show:
   - `starting` (0 %)
   - `materialising_assets` (5 %)
   - `composing` (10 %)
   - `rendering` (20 % → 95 %) — multiple log lines from hyperframes stdout/stderr
   - `finalizing` (95 %)
   - `done` (100 %) with the output path logged.

9. **Verify output file:**
   - Default location: `%LOCALAPPDATA%\Trispr Flow\videos\<job_id>_<HHMMSS>.mp4`
   - Open in VLC or Windows Media Player — the MP4 plays end-to-end.

10. **Verify the in-app player:** After render completion, the "Last render" section shows the video in an inline `<video>` player.

11. **Verify "Open Output Folder":** Click the button → Windows Explorer opens the videos directory.

12. **Verify process cleanup:**
    - During render, Task Manager should show exactly one `node.exe` child process under `Trispr Flow`.
    - After completion, that `node.exe` is gone.
    - Open `%LOCALAPPDATA%\Trispr Flow\video_jobs\` — the job's workdir should be deleted after successful render.

13. **Verify no black console windows:** At no point during render should a black `cmd.exe` or `node.exe` console window flash. `CREATE_NO_WINDOW` flag must apply.

## Known Phase 1a limitations (documented, not bugs)

- **History entry picker is a stub** — "Add History Entry..." opens a `window.prompt` for an entry_id rather than a proper picker. Phase 1b adds the picker.
- **Queue drag-reorder not yet wired** — items keep the order they were added. Remove + re-add to reorder.
- **Clipboard-paste not yet wired** — planned for Phase 1b.
- **Diagram and game_viz styles fall back to slideshow** — real Three.js/Mermaid templates arrive in Phase 4.
- **No install banner yet** — if Node isn't on PATH, render fails with an error in the log rather than a friendly install prompt. That UI arrives in Phase 1b.
- **No LLM-driven composition yet** — templates are static. Phase 2 adds the composer.

## If render fails

Common causes and remedies:

| Error message pattern                                           | Cause                              | Fix                                                              |
|-----------------------------------------------------------------|------------------------------------|------------------------------------------------------------------|
| `Node binary not found`                                         | System Node not on PATH            | Install Node 22+ or set `TRISPR_NODE_BINARY` env var.            |
| `hyperframes install not found`                                 | hyperframes not installed          | Run the `npm install hyperframes` preconditions step.            |
| `hyperframes CLI entry not found`                               | `node_modules/hyperframes/...` missing | Re-run `npm install hyperframes` in the hyperframes cwd.     |
| `Configured node_binary_path does not exist`                    | Stale settings override            | Clear the field in settings.json or set to a real path.          |
| `Render finished but output file not found`                     | hyperframes CLI flags mismatch     | Inspect the stderr tail in the progress log — adjust `run_hyperframes_render` arg form. |
| `Asset missing at render time`                                  | User moved/deleted a source file   | Re-add the file and retry.                                       |

## Sign-off checklist

- [ ] Video tab appears and is clickable
- [ ] Drag-and-drop adds files with correct kind classification
- [ ] Unsupported extensions produce clear errors
- [ ] Render completes successfully and produces a playable MP4
- [ ] Progress events stream during render
- [ ] Inline video player shows the result
- [ ] Output folder opens on click
- [ ] Job workdir cleans up after success
- [ ] No console windows flash during render
- [ ] No orphaned `node.exe` processes after render
