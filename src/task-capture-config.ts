import { invoke } from "@tauri-apps/api/core";
import { showToast } from "./toast";
import type { TaskCaptureRoute, TaskCaptureSettings } from "./types";

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function createEmptyRoute(): TaskCaptureRoute {
  return {
    label: "",
    keywords: [],
    endpoint: "",
    confluence_page_id: "",
  };
}

function cloneSettings(settings: TaskCaptureSettings): TaskCaptureSettings {
  return {
    routes: settings.routes.map((route) => ({
      label: route.label,
      keywords: [...route.keywords],
      endpoint: route.endpoint,
      confluence_page_id: route.confluence_page_id,
    })),
    match_mode: settings.match_mode,
    ai_refinement_enabled: settings.ai_refinement_enabled,
    refinement_prompt: settings.refinement_prompt,
  };
}

function ensureAtLeastOneRoute(settings: TaskCaptureSettings): TaskCaptureSettings {
  if (settings.routes.length > 0) return settings;
  return {
    ...settings,
    routes: [createEmptyRoute()],
  };
}

export async function renderTaskCaptureTab(): Promise<void> {
  const container = document.getElementById("task-capture-panel-body");
  if (!container) return;

  let loadedSettings: TaskCaptureSettings;
  try {
    loadedSettings = await invoke<TaskCaptureSettings>("get_task_capture_settings");
  } catch (error) {
    container.innerHTML = `<p class="field-hint" style="color:var(--c-error)">${escapeHtml(`Failed to load settings: ${String(error)}`)}</p>`;
    return;
  }

  let working = ensureAtLeastOneRoute(cloneSettings(loadedSettings));
  let matchModeSelect: HTMLSelectElement | null = null;
  let aiRefinementToggle: HTMLInputElement | null = null;
  let refinementPromptField: HTMLLabelElement | null = null;
  let refinementPromptInput: HTMLTextAreaElement | null = null;

  const collectCurrentSettings = (): TaskCaptureSettings => {
    const routeCards = Array.from(
      container.querySelectorAll<HTMLElement>("[data-task-capture-route]")
    );
    const routes = routeCards.map((card) => {
      const label = card.querySelector<HTMLInputElement>('[data-field="label"]');
      const keywords = card.querySelector<HTMLTextAreaElement>('[data-field="keywords"]');
      const endpoint = card.querySelector<HTMLInputElement>('[data-field="endpoint"]');
      const confluencePageId = card.querySelector<HTMLInputElement>('[data-field="confluence_page_id"]');
      return {
        label: label?.value.trim() ?? "",
        keywords: (keywords?.value ?? "")
          .split(/\r?\n/g)
          .map((value) => value.trim())
          .filter((value) => value.length > 0),
        endpoint: endpoint?.value.trim() ?? "",
        confluence_page_id: confluencePageId?.value.trim() ?? "",
      };
    });

    return ensureAtLeastOneRoute({
      routes: routes.length > 0 ? routes : [createEmptyRoute()],
      match_mode: matchModeSelect?.value === "exact" ? "exact" : "contains",
      ai_refinement_enabled: aiRefinementToggle?.checked ?? true,
      refinement_prompt: refinementPromptInput?.value ?? "",
    });
  };

  const syncPromptVisibility = () => {
    if (!refinementPromptField || !aiRefinementToggle) return;
    refinementPromptField.style.display = aiRefinementToggle.checked ? "" : "none";
  };

  const render = () => {
    working = ensureAtLeastOneRoute(working);
    container.innerHTML = "";

    const root = document.createElement("div");
    root.className = "panel-grid";

    const routesSection = document.createElement("div");
    routesSection.className = "field span-2";
    const routesTitle = document.createElement("span");
    routesTitle.className = "field-label";
    routesTitle.textContent = "Routes";
    const routesHint = document.createElement("span");
    routesHint.className = "field-hint";
    routesHint.textContent = "One route per voice target. Keywords are matched against command text.";
    const routesList = document.createElement("div");
    routesList.style.display = "grid";
    routesList.style.gap = "12px";
    routesList.style.marginTop = "10px";

    working.routes.forEach((route, index) => {
      const routeCard = document.createElement("div");
      routeCard.dataset.taskCaptureRoute = String(index);
      routeCard.style.border = "1px solid var(--border-soft, rgba(255,255,255,0.12))";
      routeCard.style.borderRadius = "14px";
      routeCard.style.padding = "14px";
      routeCard.style.background = "var(--panel-raised, rgba(255,255,255,0.02))";

      const routeGrid = document.createElement("div");
      routeGrid.className = "panel-grid";

      const labelField = document.createElement("label");
      labelField.className = "field";
      const labelText = document.createElement("span");
      labelText.className = "field-label";
      labelText.textContent = "Label";
      const labelInput = document.createElement("input");
      labelInput.type = "text";
      labelInput.value = route.label;
      labelInput.dataset.field = "label";
      labelInput.placeholder = "Agenda";
      labelField.append(labelText, labelInput);

      const pageField = document.createElement("label");
      pageField.className = "field";
      const pageText = document.createElement("span");
      pageText.className = "field-label";
      pageText.textContent = "Confluence Page ID";
      const pageInput = document.createElement("input");
      pageInput.type = "text";
      pageInput.value = route.confluence_page_id;
      pageInput.dataset.field = "confluence_page_id";
      pageInput.placeholder = "Optional";
      const pageHint = document.createElement("span");
      pageHint.className = "field-hint";
      pageHint.textContent = "Optional metadata for future routing.";
      pageField.append(pageText, pageInput, pageHint);

      const endpointField = document.createElement("label");
      endpointField.className = "field span-2";
      const endpointText = document.createElement("span");
      endpointText.className = "field-label";
      endpointText.textContent = "Endpoint URL";
      const endpointInput = document.createElement("input");
      endpointInput.type = "text";
      endpointInput.value = route.endpoint;
      endpointInput.dataset.field = "endpoint";
      endpointInput.placeholder = "http://127.0.0.1:8177/agenda/add";
      const endpointHint = document.createElement("span");
      endpointHint.className = "field-hint";
      endpointHint.textContent = "POST target. Payload shape: { \"text\": \"...\" }";
      endpointField.append(endpointText, endpointInput, endpointHint);

      const keywordsField = document.createElement("label");
      keywordsField.className = "field span-2";
      const keywordsText = document.createElement("span");
      keywordsText.className = "field-label";
      keywordsText.textContent = "Keywords";
      const keywordsInput = document.createElement("textarea");
      keywordsInput.dataset.field = "keywords";
      keywordsInput.rows = Math.max(4, route.keywords.length || 4);
      keywordsInput.value = route.keywords.join("\n");
      keywordsInput.placeholder = "erinnere mich\nadd to my agenda";
      const keywordsHint = document.createElement("span");
      keywordsHint.className = "field-hint";
      keywordsHint.textContent = "One keyword or phrase per line.";
      keywordsField.append(keywordsText, keywordsInput, keywordsHint);

      const actionsRow = document.createElement("div");
      actionsRow.className = "field span-2";
      actionsRow.style.display = "flex";
      actionsRow.style.gap = "10px";
      actionsRow.style.flexWrap = "wrap";

      const testBtn = document.createElement("button");
      testBtn.type = "button";
      testBtn.className = "hotkey-record-btn";
      testBtn.textContent = "Test Connection";
      testBtn.addEventListener("click", async () => {
        testBtn.disabled = true;
        testBtn.textContent = "Testing...";
        try {
          const result = await invoke<string>("test_task_capture_endpoint", {
            endpoint: endpointInput.value,
          });
          showToast({
            type: "success",
            title: "Connection OK",
            message: result,
            duration: 3000,
          });
        } catch (error) {
          showToast({
            type: "error",
            title: "Connection Failed",
            message: String(error),
            duration: 5000,
          });
        } finally {
          testBtn.disabled = false;
          testBtn.textContent = "Test Connection";
        }
      });

      const removeBtn = document.createElement("button");
      removeBtn.type = "button";
      removeBtn.className = "hotkey-record-btn";
      removeBtn.textContent = "Remove Route";
      removeBtn.disabled = working.routes.length <= 1;
      removeBtn.addEventListener("click", () => {
        working = collectCurrentSettings();
        working.routes.splice(index, 1);
        working = ensureAtLeastOneRoute(working);
        render();
      });

      actionsRow.append(testBtn, removeBtn);
      routeGrid.append(
        labelField,
        pageField,
        endpointField,
        keywordsField,
        actionsRow
      );
      routeCard.appendChild(routeGrid);
      routesList.appendChild(routeCard);
    });

    const addRouteBtn = document.createElement("button");
    addRouteBtn.type = "button";
    addRouteBtn.className = "hotkey-record-btn";
    addRouteBtn.textContent = "Add Route";
    addRouteBtn.style.marginTop = "12px";
    addRouteBtn.addEventListener("click", () => {
      working = collectCurrentSettings();
      working.routes.push(createEmptyRoute());
      render();
    });

    routesSection.append(routesTitle, routesHint, routesList, addRouteBtn);

    const matchModeField = document.createElement("label");
    matchModeField.className = "field";
    const matchModeText = document.createElement("span");
    matchModeText.className = "field-label";
    matchModeText.textContent = "Match Mode";
    matchModeSelect = document.createElement("select");
    matchModeSelect.innerHTML = `
      <option value="contains">Contains (default)</option>
      <option value="exact">Exact</option>
    `;
    matchModeSelect.value = working.match_mode === "exact" ? "exact" : "contains";
    const matchModeHint = document.createElement("span");
    matchModeHint.className = "field-hint";
    matchModeHint.textContent = "Contains is more forgiving for natural speech.";
    matchModeField.append(matchModeText, matchModeSelect, matchModeHint);

    const aiField = document.createElement("div");
    aiField.className = "field toggle-with-hint";
    const aiLabel = document.createElement("label");
    aiLabel.className = "toggle-row";
    const aiText = document.createElement("span");
    aiText.className = "field-label";
    aiText.textContent = "AI Refinement";
    aiRefinementToggle = document.createElement("input");
    aiRefinementToggle.type = "checkbox";
    aiRefinementToggle.checked = working.ai_refinement_enabled;
    const aiTrack = document.createElement("span");
    aiTrack.className = "toggle-track";
    const aiThumb = document.createElement("span");
    aiThumb.className = "toggle-thumb";
    aiTrack.appendChild(aiThumb);
    aiLabel.append(aiText, aiRefinementToggle, aiTrack);
    const aiHint = document.createElement("span");
    aiHint.className = "toggle-hint";
    aiHint.textContent = "When enabled, local AI rewrites raw reminder text into a cleaner task.";
    aiField.append(aiLabel, aiHint);

    refinementPromptField = document.createElement("label");
    refinementPromptField.className = "field span-2";
    const promptText = document.createElement("span");
    promptText.className = "field-label";
    promptText.textContent = "Refinement Prompt";
    refinementPromptInput = document.createElement("textarea");
    refinementPromptInput.rows = 5;
    refinementPromptInput.value = working.refinement_prompt;
    const promptHint = document.createElement("span");
    promptHint.className = "field-hint";
    promptHint.textContent = "Custom prompt passed to task refinement model.";
    refinementPromptField.append(promptText, refinementPromptInput, promptHint);

    aiRefinementToggle.addEventListener("change", () => {
      syncPromptVisibility();
    });

    const actionsFooter = document.createElement("div");
    actionsFooter.className = "field span-2";
    actionsFooter.style.display = "flex";
    actionsFooter.style.justifyContent = "flex-end";

    const saveBtn = document.createElement("button");
    saveBtn.type = "button";
    saveBtn.className = "hotkey-record-btn";
    saveBtn.textContent = "Save";
    saveBtn.addEventListener("click", async () => {
      const nextSettings = collectCurrentSettings();
      saveBtn.disabled = true;
      saveBtn.textContent = "Saving...";
      try {
        await invoke("save_task_capture_settings", {
          taskCaptureSettings: nextSettings,
        });
        working = nextSettings;
        showToast({
          type: "success",
          title: "Saved",
          message: "Task Capture settings updated.",
          duration: 2600,
        });
      } catch (error) {
        showToast({
          type: "error",
          title: "Save failed",
          message: String(error),
          duration: 5000,
        });
      } finally {
        saveBtn.disabled = false;
        saveBtn.textContent = "Save";
      }
    });

    actionsFooter.appendChild(saveBtn);

    root.append(
      routesSection,
      matchModeField,
      aiField,
      refinementPromptField,
      actionsFooter
    );
    container.appendChild(root);
    syncPromptVisibility();
  };

  render();
}
