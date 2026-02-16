// Toast notification system tests
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { showToast, dismissToast, showErrorToast } from "../toast";
import type { ToastOptions, AppErrorType } from "../types";

describe("Toast System", () => {
  let container: HTMLElement;

  beforeEach(() => {
    // Setup DOM
    container = document.createElement("div");
    container.id = "toast-container";
    document.body.appendChild(container);

    // Mock timers
    vi.useFakeTimers();
  });

  afterEach(() => {
    // Cleanup
    document.body.removeChild(container);
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  describe("showToast", () => {
    it("should create a toast with title and message", () => {
      const options: ToastOptions = {
        type: "info",
        title: "Test Title",
        message: "Test Message",
      };

      const id = showToast(options);

      expect(id).toBeTruthy();
      const toast = document.getElementById(id!);
      expect(toast).toBeTruthy();
      expect(toast?.className).toContain("toast");
      expect(toast?.className).toContain("info");

      const title = toast?.querySelector(".toast-title");
      expect(title?.textContent).toBe("Test Title");

      const message = toast?.querySelector(".toast-message");
      expect(message?.textContent).toBe("Test Message");
    });

    it("should use correct icon for each toast type", () => {
      const types = ["error", "success", "warning", "info"] as const;
      const expectedIcons = {
        error: "❌",
        success: "✅",
        warning: "⚠️",
        info: "ℹ️",
      };

      types.forEach((type) => {
        const id = showToast({
          type,
          title: "Test",
          message: "Test",
        });

        const icon = document.querySelector(`#${id} .toast-icon`);
        expect(icon?.textContent).toBe(expectedIcons[type]);
      });
    });

    it("should use custom icon when provided", () => {
      const id = showToast({
        type: "info",
        title: "Test",
        message: "Test",
        icon: "🎉",
      });

      const icon = document.querySelector(`#${id} .toast-icon`);
      expect(icon?.textContent).toBe("🎉");
    });

    it("should auto-remove toast after duration", () => {
      const id = showToast({
        type: "info",
        title: "Test",
        message: "Test",
        duration: 1000,
      });

      expect(document.getElementById(id!)).toBeTruthy();

      // Fast-forward past duration
      vi.advanceTimersByTime(1000);

      // Fast-forward past removal animation
      vi.advanceTimersByTime(200);

      expect(document.getElementById(id!)).toBeFalsy();
    });

    it("should not auto-remove when duration is 0", () => {
      const id = showToast({
        type: "info",
        title: "Test",
        message: "Test",
        duration: 0,
      });

      vi.advanceTimersByTime(10000);
      expect(document.getElementById(id!)).toBeTruthy();
    });

    it("should create action button when actionLabel and onAction provided", () => {
      const onAction = vi.fn();

      const id = showToast({
        type: "info",
        title: "Test",
        message: "Test",
        actionLabel: "Click Me",
        onAction,
      });

      const actionBtn = document.querySelector(`#${id} .toast-action`) as HTMLButtonElement;
      expect(actionBtn).toBeTruthy();
      expect(actionBtn?.textContent).toBe("Click Me");

      actionBtn?.click();
      expect(onAction).toHaveBeenCalled();
    });

    it("should dismiss toast after action when actionDismiss is not false", async () => {
      const onAction = vi.fn();

      const id = showToast({
        type: "info",
        title: "Test",
        message: "Test",
        actionLabel: "Click Me",
        onAction,
        actionDismiss: true,
      });

      const actionBtn = document.querySelector(`#${id} .toast-action`) as HTMLButtonElement;
      actionBtn?.click();

      // Wait for action to complete
      await vi.runAllTimersAsync();

      expect(document.getElementById(id!)).toBeFalsy();
    });

    it("should have close button that dismisses toast", () => {
      const id = showToast({
        type: "info",
        title: "Test",
        message: "Test",
      });

      const closeBtn = document.querySelector(`#${id} .toast-close`) as HTMLButtonElement;
      expect(closeBtn).toBeTruthy();

      closeBtn?.click();

      // Fast-forward past removal animation
      vi.advanceTimersByTime(200);

      expect(document.getElementById(id!)).toBeFalsy();
    });

    it("should return null if container does not exist", () => {
      // Remove container
      document.body.removeChild(container);

      const id = showToast({
        type: "info",
        title: "Test",
        message: "Test",
      });

      expect(id).toBeNull();

      // Restore container for cleanup
      document.body.appendChild(container);
    });

    it("should generate unique IDs for multiple toasts", () => {
      const id1 = showToast({ type: "info", title: "1", message: "1" });
      const id2 = showToast({ type: "info", title: "2", message: "2" });

      expect(id1).not.toBe(id2);
      expect(document.getElementById(id1!)).toBeTruthy();
      expect(document.getElementById(id2!)).toBeTruthy();
    });
  });

  describe("dismissToast", () => {
    it("should remove toast by ID", () => {
      const id = showToast({
        type: "info",
        title: "Test",
        message: "Test",
      });

      dismissToast(id);

      // Fast-forward past removal animation
      vi.advanceTimersByTime(200);

      expect(document.getElementById(id!)).toBeFalsy();
    });

    it("should handle null ID gracefully", () => {
      expect(() => dismissToast(null)).not.toThrow();
    });

    it("should handle undefined ID gracefully", () => {
      expect(() => dismissToast(undefined)).not.toThrow();
    });
  });

  describe("showErrorToast", () => {
    it("should show error toast with correct title for each error type", () => {
      const errorTypes: AppErrorType["type"][] = [
        "AudioDevice",
        "Transcription",
        "Hotkey",
        "Storage",
        "Network",
        "Window",
        "Other",
      ];

      const expectedTitles: Record<string, string> = {
        AudioDevice: "Audio Device Issue",
        Transcription: "Transcription Failed",
        Hotkey: "Hotkey Problem",
        Storage: "Storage Error",
        Network: "Network Problem",
        Window: "Window Error",
        Other: "Error",
      };

      errorTypes.forEach((type) => {
        container.innerHTML = ""; // Clear previous toasts

        const error: AppErrorType = {
          type,
          message: "Test error message",
        };

        showErrorToast(error);

        const toast = container.querySelector(".toast");
        const title = toast?.querySelector(".toast-title");

        expect(title?.textContent).toBe(expectedTitles[type]);
      });
    });

    it("should include context in message when provided", () => {
      const error: AppErrorType = {
        type: "Other",
        message: "Something went wrong",
      };

      showErrorToast(error, "File upload");

      const message = container.querySelector(".toast-message");
      expect(message?.textContent).toBe("File upload: Something went wrong");
    });

    it("should use only error message when context is not provided", () => {
      const error: AppErrorType = {
        type: "Other",
        message: "Something went wrong",
      };

      showErrorToast(error);

      const message = container.querySelector(".toast-message");
      expect(message?.textContent).toBe("Something went wrong");
    });

    it("should create error-type toast", () => {
      const error: AppErrorType = {
        type: "Other",
        message: "Test",
      };

      showErrorToast(error);

      const toast = container.querySelector(".toast");
      expect(toast?.className).toContain("error");
    });

    it("should have 7-second duration", () => {
      const error: AppErrorType = {
        type: "Other",
        message: "Test",
      };

      showErrorToast(error);

      const toast = container.querySelector(".toast");
      expect(toast).toBeTruthy();

      // Should still be visible after 6 seconds
      vi.advanceTimersByTime(6000);
      expect(container.querySelector(".toast")).toBeTruthy();

      // Should be removed after 7 seconds + animation
      vi.advanceTimersByTime(1000);
      vi.advanceTimersByTime(200);
      expect(container.querySelector(".toast")).toBeFalsy();
    });
  });
});
