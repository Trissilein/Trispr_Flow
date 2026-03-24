import { vi } from "vitest";

// Global mock for Tauri APIs — jsdom test environment has no Tauri runtime.
// Without this, any test that calls wireEvents() or similar entry points will
// crash with "Cannot read properties of undefined (reading 'transformCallback')".

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
  once: vi.fn(async () => () => {}),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => ({})),
}));
