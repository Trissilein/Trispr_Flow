/**
 * Welle 4 contract: `finalize_ollama_model_update` may answer `inconclusive: true` (the check did not
 * finish: timeout, transport error, HTTP 5xx, Ollama gone). Both builds stay, and the card keeps a
 * notice with "Re-run check" and "Roll back" (`rollback_ollama_model_update(model, backup)`).
 * The backend side is mocked against that contract.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  OLD,
  hang,
  load,
  settle,
  startUpdate,
  tick,
  toastNamed,
  toastTitles,
  updateHandlers,
  useFakeClock,
  type Ctx,
  type Handler,
} from "./ollama-test-harness";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
}));

vi.mock("../toast", () => ({
  showToast: vi.fn(),
}));

const version = { current: "0.40.7" };
const BACKUP = `${OLD}.bak-1`;
const INCONCLUSIVE = { verified: false, restored: false, inconclusive: true, detail: "The check timed out after 90 s." };

beforeEach(() => {
  version.current = "0.40.7";
});

afterEach(() => {
  vi.useRealTimers();
});

const button = (ctx: Ctx, action: string) =>
  ctx.cardOf(OLD)?.querySelector<HTMLButtonElement>(`button[data-action="${action}"]`) ?? null;

/** An update whose final check is inconclusive; resolves when the card shows the notice. */
async function inconclusiveUpdate(extra: Record<string, Handler> = {}, finalize: Handler = () => INCONCLUSIVE) {
  const ctx = await load(updateHandlers(version, { finalize_ollama_model_update: finalize, ...extra }), { settingsModel: OLD });
  await ctx.boot();
  await startUpdate(ctx);
  await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
  ctx.emit("ollama:pull-complete", { model: OLD });
  await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Update check inconclusive"));
  await vi.waitFor(() => expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false));
  await ctx.render();
  return ctx;
}

describe("an inconclusive check", () => {
  it("keeps both builds, says so, and leaves a notice on the card with the two ways out", async () => {
    const ctx = await inconclusiveUpdate();

    const toast = toastNamed(ctx.showToast, "Update check inconclusive");
    expect(toast.type).toBe("warning");
    expect(toast.message).toContain("could not be checked");
    expect(toast.message).toContain("The check timed out after 90 s.");
    expect(toastTitles(ctx.showToast)).not.toContain("Model updated");
    expect(toastTitles(ctx.showToast)).not.toContain("Update check failed");
    expect(toastTitles(ctx.showToast)).not.toContain("Update rolled back");

    const card = ctx.cardOf(OLD)!;
    expect(card.textContent).toContain("Check pending");
    const notice = card.querySelector(".qwen-card-hint--check")!.textContent!;
    expect(notice).toContain("The check timed out after 90 s.");
    expect(notice).toContain(BACKUP);
    expect(notice).toMatch(/lost when the app restarts/);
    expect(button(ctx, "recheck")!.textContent).toBe("Re-run check");
    expect(button(ctx, "rollback")!.textContent).toBe("Roll back");
    expect(button(ctx, "recheck")!.disabled).toBe(false);
    // A second update would make a second backup of an unchecked build.
    expect(button(ctx, "update")).toBeNull();
    expect(ctx.callsOf("rollback_ollama_model_update")).toHaveLength(0);
  });

  it("keeps the notice across renders and list refreshes, and drops it when the model is gone", async () => {
    let installed = true;
    const ctx = await inconclusiveUpdate({
      fetch_ollama_models_with_size: () => (installed ? [{ name: OLD, size_bytes: 9_608_350_718, digest: "c".repeat(64) }] : []),
    });
    for (let i = 0; i < 3; i += 1) {
      await ctx.mod.refreshOllamaInstalledModels();
      await ctx.render();
      expect(ctx.cardOf(OLD)!.querySelector(".qwen-card-hint--check")).not.toBeNull();
    }
    installed = false;
    await ctx.mod.refreshOllamaInstalledModels();
    installed = true;
    await ctx.mod.refreshOllamaInstalledModels();
    await ctx.render();
    expect(ctx.cardOf(OLD)!.querySelector(".qwen-card-hint--check")).toBeNull();
  });

  it("an unchecked build that is also unreachable (finalize throws) leaves the same way out", async () => {
    const ctx = await load(
      updateHandlers(version, {
        finalize_ollama_model_update: () => {
          throw "model busy";
        },
      }),
      { settingsModel: OLD }
    );
    await ctx.boot();
    await startUpdate(ctx);
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    ctx.emit("ollama:pull-complete", { model: OLD });
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Update could not be finished"));
    expect(toastNamed(ctx.showToast, "Update could not be finished").message).toContain(`"${BACKUP}" was kept`);
    await vi.waitFor(() => expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false));
    await ctx.render();
    expect(button(ctx, "recheck")).not.toBeNull();
    expect(button(ctx, "rollback")).not.toBeNull();
    expect(ctx.cardOf(OLD)!.querySelector(".qwen-card-hint--check")!.textContent).toContain("model busy");
  });

  it("after a failed download an inconclusive check names both and keeps the backup", async () => {
    const ctx = await load(updateHandlers(version, { finalize_ollama_model_update: () => INCONCLUSIVE }), { settingsModel: OLD });
    await ctx.boot();
    await startUpdate(ctx);
    await vi.waitFor(() => expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1));
    ctx.emit("ollama:pull-error", { model: OLD, error: "Connection failed: reset" });
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Update check inconclusive"));
    expect(toastTitles(ctx.showToast)).not.toContain("Update failed");
    const message = toastNamed(ctx.showToast, "Update check inconclusive").message;
    expect(message).toContain("Connection failed: reset");
    await vi.waitFor(() => expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false));
    await ctx.render();
    expect(button(ctx, "rollback")).not.toBeNull();
  });
});

describe("Re-run check", () => {
  it("calls finalize again with the same backup and ends the notice on a clear pass", async () => {
    let answer: unknown = INCONCLUSIVE;
    const ctx = await inconclusiveUpdate({}, () => answer);
    answer = { verified: true, restored: false, detail: null };

    button(ctx, "recheck")!.click();
    await vi.waitFor(() => expect(ctx.callsOf("finalize_ollama_model_update")).toHaveLength(2));
    expect(ctx.callsOf("finalize_ollama_model_update")[1].args).toEqual({ model: OLD, backup: BACKUP });
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Model updated"));
    await vi.waitFor(() => expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false));
    await ctx.render();
    expect(ctx.cardOf(OLD)!.querySelector(".qwen-card-hint--check")).toBeNull();
    expect(button(ctx, "recheck")).toBeNull();
    // No new download for a re-run.
    expect(ctx.callsOf("pull_ollama_model")).toHaveLength(1);
    expect(ctx.callsOf("backup_ollama_model")).toHaveLength(1);
  });

  it("stays pending when it is inconclusive again, and when finalize throws", async () => {
    let step = 0;
    const ctx = await inconclusiveUpdate({}, () => {
      step += 1;
      if (step === 1 || step === 2) return INCONCLUSIVE;
      throw "Ollama is not reachable";
    });
    button(ctx, "recheck")!.click();
    await vi.waitFor(() => expect(ctx.showToast.mock.calls.filter((call) => call[0].title === "Update check inconclusive")).toHaveLength(2));
    await vi.waitFor(() => expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false));
    await ctx.render();
    expect(button(ctx, "recheck")).not.toBeNull();

    button(ctx, "recheck")!.click();
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Update could not be finished"));
    await vi.waitFor(() => expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false));
    await ctx.render();
    expect(button(ctx, "recheck")).not.toBeNull();
    expect(button(ctx, "rollback")).not.toBeNull();
  });

  it("a content failure on re-run restores and ends the notice", async () => {
    let answer: unknown = INCONCLUSIVE;
    const ctx = await inconclusiveUpdate({}, () => answer);
    answer = { verified: false, restored: true, detail: "Output missed 5 of 12 words." };
    button(ctx, "recheck")!.click();
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Update rolled back"));
    await vi.waitFor(() => expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false));
    await ctx.render();
    expect(ctx.cardOf(OLD)!.querySelector(".qwen-card-hint--check")).toBeNull();
  });

  it("locks the card's other buttons while it runs and a hung check is given up on", async () => {
    let hangNow = false;
    const ctx = await inconclusiveUpdate({}, () => (hangNow ? hang() : INCONCLUSIVE));
    hangNow = true;
    useFakeClock();
    button(ctx, "recheck")!.click();
    await tick(200);
    expect(ctx.mod.isOllamaModelOperationRunning(OLD)).toBe(true);
    expect(ctx.cardOf(OLD)!.textContent).toContain("Checking");
    expect(button(ctx, "recheck")).toBeNull();
    expect(button(ctx, "rollback")).toBeNull();

    await tick(6 * 60_000);
    expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false);
    expect(toastTitles(ctx.showToast)).toContain("Update could not be finished");
    await tick(500);
    expect(button(ctx, "recheck")).not.toBeNull(); // still pending, still recoverable
  });
});

describe("Roll back", () => {
  it("asks first, naming the backup copy; cancelling changes nothing", async () => {
    const ctx = await inconclusiveUpdate({ rollback_ollama_model_update: () => ({ restored: true, detail: null }) });
    button(ctx, "rollback")!.click();
    await settle();
    const text = await ctx.confirmDialog("cancel");
    expect(text).toContain(BACKUP);
    expect(text).toContain("not checked successfully");
    expect(ctx.callsOf("rollback_ollama_model_update")).toHaveLength(0);
    await ctx.render();
    expect(button(ctx, "rollback")).not.toBeNull();
  });

  it("two quick clicks open one confirmation", async () => {
    const ctx = await inconclusiveUpdate({ rollback_ollama_model_update: () => ({ restored: true }) });
    const rollback = button(ctx, "rollback")!;
    rollback.click();
    rollback.click();
    await settle();
    expect(ctx.dialogs()).toHaveLength(1);
    await ctx.confirmDialog("cancel");
  });

  it("calls rollback_ollama_model_update(model, backup) and ends the notice when restored", async () => {
    const ctx = await inconclusiveUpdate({ rollback_ollama_model_update: () => ({ restored: true, detail: null }) });
    button(ctx, "rollback")!.click();
    await settle();
    await ctx.confirmDialog("confirm");

    await vi.waitFor(() => expect(ctx.callsOf("rollback_ollama_model_update")).toHaveLength(1));
    expect(ctx.callsOf("rollback_ollama_model_update")[0].args).toEqual({ model: OLD, backup: BACKUP });
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Rolled back"));
    await vi.waitFor(() => expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false));
    await ctx.render();
    expect(ctx.cardOf(OLD)!.querySelector(".qwen-card-hint--check")).toBeNull();
    expect(button(ctx, "recheck")).toBeNull();
  });

  it.each([
    ["restored: false", () => ({ restored: false, detail: "copy failed" })],
    ["a thrown error", () => {
      throw "copy failed";
    }],
    ["an unreadable answer", () => null],
  ])("keeps the notice and names the backup when the backend answers with %s", async (_label, answer) => {
    const ctx = await inconclusiveUpdate({ rollback_ollama_model_update: answer as Handler });
    button(ctx, "rollback")!.click();
    await settle();
    await ctx.confirmDialog("confirm");
    await vi.waitFor(() => expect(toastTitles(ctx.showToast)).toContain("Roll back failed"));
    expect(toastNamed(ctx.showToast, "Roll back failed").message).toContain(`"${BACKUP}" is still there`);
    await vi.waitFor(() => expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false));
    await ctx.render();
    expect(button(ctx, "rollback")).not.toBeNull();
    expect(button(ctx, "recheck")).not.toBeNull();
  });

  it("a hung rollback is given up on and the notice stays", async () => {
    const ctx = await inconclusiveUpdate({ rollback_ollama_model_update: hang });
    useFakeClock();
    button(ctx, "rollback")!.click();
    await tick(200);
    await ctx.confirmDialog("confirm");
    await tick(200);
    expect(ctx.mod.isOllamaModelOperationRunning(OLD)).toBe(true);
    await tick(6 * 60_000);
    expect(ctx.mod.isOllamaModelOperationRunning()).toBe(false);
    expect(toastTitles(ctx.showToast)).toContain("Roll back failed");
    await tick(500);
    expect(button(ctx, "rollback")).not.toBeNull();
  });
});
