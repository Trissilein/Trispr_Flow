# Block G Implementation Guide (Opus Sprint)

> Canonical priority/dependency source: `ROADMAP.md`.  
> This file is a detailed implementation handover for Block G only.

**For**: Next developer on v0.7.0 AI Fallback feature
**Model**: Claude Opus
**Duration**: 2-3 weeks
**Status**: Ready to start
**Last updated**: 2026-02-16

---

## Quick Context

v0.6.0 is complete and released. v0.7.0 planning (Block F) is done. You're starting **Block G** — the architecture and UI implementation phase for multi-provider AI Fallback.

**What this does**: Replaces single "Cloud Fallback" (Claude-only) with support for Claude, OpenAI, and Gemini. Users can choose their preferred provider and model.

**Why it matters**: Users don't want vendor lock-in. They want choice and control over cost.

---

## What's Already Done (Block F — Haiku)

✅ **Architecture designed**:
- Rust provider trait pattern defined
- ProviderFactory design documented
- Error handling strategy planned
- TypeScript interfaces designed

✅ **UI mockups created**:
- AI Fallback expander in Post-Processing panel
- API key setup dialog
- Custom prompt editor modal

✅ **Decisions locked in** (see DECISIONS.md):
- DEC-023: Terminology = "AI Fallback"
- DEC-024: Location = Post-Processing panel expander
- DEC-025: Sequence = Local Rules → AI Fallback

✅ **Documentation complete**:
- [V0.7.0_ARCHITECTURE.md](V0.7.0_ARCHITECTURE.md) — Full technical spec
- [V0.7.0_PLAN.md](V0.7.0_PLAN.md) — Planning doc with all decisions

---

## Your Tasks (Block G — 3 Tasks)

### Task 31: Multi-Provider Architecture

**What to build**:
1. **Provider trait** (Rust)
   - Methods: `refine_transcript()`, `estimate_cost()`, `available_models()`, `name()`
   - Error handling with unified AIError enum
   - Token usage tracking (input/output)

2. **ProviderFactory** (Rust)
   - Factory pattern: creates correct provider instance based on config
   - Error handling for unknown providers

3. **Models** (Rust)
   - `AIFallbackSettings` struct
   - `RefinementResult` struct
   - `TokenUsage` struct
   - `AIError` enum with variants

**File locations**:
- `src-tauri/src/ai_fallback/mod.rs` — module root
- `src-tauri/src/ai_fallback/provider.rs` — trait definition
- `src-tauri/src/ai_fallback/models.rs` — data types
- `src-tauri/src/ai_fallback/error.rs` — error handling

**Success criteria**:
- ✅ Trait compiles and can be implemented
- ✅ Factory creates providers correctly
- ✅ All error types handled
- ✅ Unit tests for factory

**Reference**: See V0.7.0_ARCHITECTURE.md lines 65-117 for trait definitions

---

### Task 36: Settings Migration + Data Model

**What to build**:
1. **Settings schema** (Rust in state.rs)
   - New `AIFallbackSettings` struct in Settings
   - Provider metadata (available_models, api_key_stored flag)
   - Migration function from v0.6.0 cloud_fallback

2. **Settings persistence**
   - Load/save AIFallbackSettings from JSON
   - Handle provider-specific configuration

3. **API key management** (Windows/macOS priority)
   - Windows: Store in Credential Manager (DPAPI encryption)
   - macOS: Store in Keychain
   - Linux/fallback: Store in settings.json with warning

**File locations**:
- `src-tauri/src/state.rs` — Settings struct + migration logic
- `src-tauri/src/ai_fallback/keyring.rs` — (new) Key storage helpers

**Success criteria**:
- ✅ Settings load and save correctly
- ✅ Migration from v0.6.0 cloud_fallback works
- ✅ API keys stored securely (Windows/macOS)
- ✅ Tests verify migration logic

**Reference**: See V0.7.0_ARCHITECTURE.md lines 154-207 for settings schema

---

### Task 37: Configuration UI

**What to build**:
1. **AI Fallback expander** (TypeScript)
   - Lives in Post-Processing panel
   - Toggle to enable/disable AI Fallback
   - Provider selector (radio buttons: Claude / OpenAI / Gemini)

2. **Model selector** (TypeScript)
   - Dropdown for model selection per provider
   - Dynamic: fetched from API or hardcoded defaults

3. **API key setup dialog** (TypeScript)
   - Modal dialog to paste API key
   - "Test Connection" button (calls Tauri command)
   - Success/error messages

4. **Advanced options** (TypeScript)
   - Temperature slider (0.0-1.0)
   - Max tokens selector
   - Custom prompt toggle + editor button

5. **Cost estimate display** (TypeScript)
   - Show estimated cost per refinement (e.g., "$0.01")
   - Calculated locally (no API call)

**File locations**:
- `src/panels/ai_fallback.ts` — (new) UI module
- `src/settings.ts` — integration point (add expander to renderSettings)
- `src/types.ts` — TypeScript types

**Success criteria**:
- ✅ UI matches mockups from V0.7.0_PLAN.md
- ✅ Expander toggles open/closed
- ✅ Provider selector works
- ✅ API key setup dialog functional
- ✅ Test Connection button wired to Tauri command (stub for now)
- ✅ Settings persist to JSON

**Reference**: See V0.7.0_PLAN.md lines 225-257 for UI mockups

---

## Tauri Commands (Setup for Block H)

Add to `src-tauri/src/lib.rs`:

```rust
#[tauri::command]
async fn test_provider_connection(
    provider: String,
    api_key: String,
) -> Result<bool, String> {
    // Stub for now — will be implemented in Block H
    // Task 32-34: Each provider implements this
    Ok(true)
}

#[tauri::command]
async fn fetch_available_models(
    provider: String,
    api_key: String,
) -> Result<Vec<String>, String> {
    // Stub for now — will be implemented in Block H
    Ok(vec![])
}

#[tauri::command]
async fn refine_transcript(
    state: tauri::State<'_, AppState>,
    transcript: String,
) -> Result<RefinementResult, String> {
    // Stub for now — will be implemented in Block H
    Err("Not yet implemented".to_string())
}
```

These stubs let the UI work while waiting for Block H (provider implementations).

---

## Dependencies

Add to `Cargo.toml`:

```toml
# AI provider libraries (used in Block H)
reqwest = { version = "0.11", features = ["json", "stream"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Key storage (Windows)
[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.51", features = ["Win32_Security_Cryptography"] }

# Key storage (macOS)
[target.'cfg(target_os = "macos")'.dependencies]
security-framework = "2.9"
```

---

## Testing Strategy

### Unit tests
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_provider_factory_creates_claude() { }

    #[test]
    fn test_invalid_provider_error() { }

    #[test]
    fn test_settings_migration_cloud_fallback_to_ai_fallback() { }
}
```

### Frontend tests
- Verify expander toggle state
- Verify provider selector updates model dropdown
- Verify API key dialog opens/closes
- Verify settings persist to JSON

---

## Merge Strategy

After completing all 3 tasks:
1. Create branch: `feature/block-g-architecture`
2. Commit with message: `Block G: Multi-provider AI Fallback architecture (Tasks 31, 36, 37)`
3. Create PR with title: `v0.7.0 Block G: Multi-provider architecture and config UI`
4. Link to Tasks 31, 36, 37 in description
5. After merge, update TASK_SCHEDULE.md to mark Block G complete

---

## What Happens After (Block H — Sonnet)

Once Block G is merged, Block H starts:
- **Task 32**: OpenAI client implementation
- **Task 33**: Claude client implementation
- **Task 34**: Gemini client implementation
- **Task 35**: Custom prompt strategy
- **Task 38**: E2E tests

Block H fills in the provider implementations and the stubs you create in Block G.

---

## Debugging Checklist

When something doesn't work:

- ✅ **Settings not saving?** Check `state.rs` serialization
- ✅ **UI not updating?** Check `wireEvents()` in settings.ts
- ✅ **Trait errors?** Check async/Send+Sync bounds
- ✅ **Compile errors?** Run `cargo check` for detailed error messages
- ✅ **TypeScript errors?** Run `npm run build` to catch type mismatches

---

## Key Files to Read

Before starting, read these in order:

1. 📖 [V0.7.0_ARCHITECTURE.md](V0.7.0_ARCHITECTURE.md) — Full technical spec (400+ lines)
2. 📖 [V0.7.0_PLAN.md](V0.7.0_PLAN.md) — Planning and UI mockups
3. 📖 [docs/DECISIONS.md](../DECISIONS.md) — DEC-023, DEC-024, DEC-025
4. 🔧 [src-tauri/src/state.rs](../src-tauri/src/state.rs) — Current settings structure
5. 🔧 [src/settings.ts](../src/settings.ts) — UI structure to understand where to add expander

---

## Communication

If you have questions:
1. Check DECISIONS.md first (most questions are answered there)
2. Check V0.7.0_ARCHITECTURE.md (technical spec)
3. Check existing similar code (e.g., Post-Processing panel for UI pattern)
4. Ask in Issues or PRs with context

---

## Good Luck! 🚀

You've got clear specs, working examples from v0.6.0, and stubbed Tauri commands. Block G is the architectural foundation — get it right here, and Block H (provider implementations) will be straightforward.

Start with **Task 31** (provider trait) since everything depends on it.

**Estimated time**: 2-3 weeks with parallel TypeScript/Rust work.

---

**Next person**: When you finish Block G, update this file with what actually took time and what was unclear. Make it easier for the next contributor.
