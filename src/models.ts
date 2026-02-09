// Model management and rendering

import { invoke } from "@tauri-apps/api/core";
import type { ModelInfo } from "./types";
import { settings, models, setModels, modelProgress } from "./state";
import * as dom from "./dom-refs";
import { getModelDescription, formatSize, formatProgress } from "./ui-helpers";
import { persistSettings } from "./settings";
import { renderHero } from "./ui-state";
import { showToast } from "./toast";

const optimizingModels = new Set<string>();

export function renderModels() {
  if (!dom.modelList) return;
  dom.modelList.innerHTML = "";

  const installedModels = models.filter((model) => model.installed);
  const availableModels = models.filter((model) => !model.installed && model.available);

  let activeModel = settings ? installedModels.find((model) => model.id === settings?.model) : undefined;
  if (settings && installedModels.length && !activeModel) {
    settings.model = installedModels[0].id;
    persistSettings();
    activeModel = installedModels[0];
  }

  const renderGroup = (container: HTMLElement, group: ModelInfo[], emptyText: string) => {
    if (!group.length) {
      container.innerHTML = `<div class="model-item"><div class="model-name">${emptyText}</div></div>`;
      return;
    }

    group.forEach((model) => {
      const item = document.createElement("div");
      item.className = "model-item";
      const isActive = settings?.model === model.id;
      if (isActive) {
        item.classList.add("selected");
      }
      if (!model.installed) {
        item.classList.add("model-item--available");
      }
      // Removed click-to-select behavior - only Apply button switches models
      // This prevents confusion between selecting and applying

      const header = document.createElement("div");
      header.className = "model-header";

      const name = document.createElement("div");
      name.className = "model-name";
      name.textContent = model.label;

      const size = document.createElement("div");
      size.className = "model-size";
      size.textContent = model.size_mb > 0 ? formatSize(model.size_mb) : "Size unknown";

      header.appendChild(name);
      header.appendChild(size);

      const meta = document.createElement("div");
      meta.className = "model-meta";
      const source = model.source ? ` • ${model.source}` : "";
      meta.textContent = `${model.file_name}${source}`;

      const description = document.createElement("div");
      description.className = "model-desc";
      description.textContent = getModelDescription(model);

      const pathLine = document.createElement("div");
      pathLine.className = "model-meta";
      if (model.path) {
        pathLine.textContent = model.path;
      }

      const status = document.createElement("div");
      status.className = `model-status ${model.installed ? "downloaded" : "available"}${
        isActive ? " active" : ""
      }`;
      status.textContent = model.installed
        ? isActive
          ? "Active"
          : model.removable
            ? "Installed"
            : "Installed (external)"
        : model.downloading
          ? "Downloading"
          : "Available";

      const actions = document.createElement("div");
      actions.className = "model-actions";

      if (model.installed) {
        // Add Apply button if model is not currently active
        if (!isActive) {
          const applyBtn = document.createElement("button");
          applyBtn.className = "btn-apply-model";
          applyBtn.innerHTML = `
            <span class="btn-apply-icon">⚡</span>
            <span class="btn-apply-text">Apply Model</span>
          `;
          applyBtn.addEventListener("click", async (event) => {
            event.stopPropagation();

            // Add loading state
            applyBtn.classList.add("is-loading");
            applyBtn.innerHTML = `
              <span class="btn-apply-spinner"></span>
              <span class="btn-apply-text">Applying...</span>
            `;
            applyBtn.disabled = true;

            try {
              await invoke("apply_model", { modelId: model.id });

              // Success state with animation
              applyBtn.classList.remove("is-loading");
              applyBtn.classList.add("is-success");
              applyBtn.innerHTML = `
                <span class="btn-apply-icon">✓</span>
                <span class="btn-apply-text">Applied!</span>
              `;

              showToast({
                title: "Model Activated",
                message: `Now using ${model.label} for transcription.`,
                type: "success",
              });

              // Re-render after brief delay to show success state
              setTimeout(() => {
                renderModels();
              }, 800);
            } catch (error) {
              // Error state
              applyBtn.classList.remove("is-loading");
              applyBtn.classList.add("is-error");
              applyBtn.innerHTML = `
                <span class="btn-apply-icon">✕</span>
                <span class="btn-apply-text">Failed</span>
              `;
              applyBtn.disabled = false;

              showToast({
                title: "Model Switch Failed",
                message: String(error),
                type: "error",
              });

              // Reset button after delay
              setTimeout(() => {
                applyBtn.classList.remove("is-error");
                applyBtn.innerHTML = `
                  <span class="btn-apply-icon">⚡</span>
                  <span class="btn-apply-text">Apply Model</span>
                `;
              }, 2000);
            }
          });
          actions.appendChild(applyBtn);
        }

        const canOptimize =
          model.removable &&
          model.file_name.endsWith(".bin") &&
          !model.file_name.includes("-q5_0");

        if (canOptimize) {
          const optimizeBtn = document.createElement("button");
          const isOptimizing = optimizingModels.has(model.id);
          optimizeBtn.textContent = isOptimizing ? "Optimizing..." : "Optimize";
          optimizeBtn.disabled = isOptimizing;
          optimizeBtn.addEventListener("click", async (event) => {
            event.stopPropagation();
            if (optimizingModels.has(model.id)) return;
            optimizingModels.add(model.id);
            renderModels();
            try {
              await invoke("quantize_model", { fileName: model.file_name, quant: "q5_0" });
              showToast({
                title: "Optimized",
                message: "Quantized model created (q5_0).",
                type: "success",
              });
              await refreshModels();
            } catch (error) {
              showToast({
                title: "Optimize failed",
                message: String(error),
                type: "error",
              });
            } finally {
              optimizingModels.delete(model.id);
              renderModels();
            }
          });
          actions.appendChild(optimizeBtn);
        }

        const removeBtn = document.createElement("button");
        const isExternal = !model.removable;
        removeBtn.textContent = isExternal ? "Remove" : "Delete";
        removeBtn.addEventListener("click", async (event) => {
          event.stopPropagation();
          try {
            if (isExternal) {
              if (!model.path) {
                showToast({
                  title: "Remove failed",
                  message: "External model path missing.",
                  type: "warning",
                });
                return;
              }
              await invoke("hide_external_model", { path: model.path });
              showToast({
                title: "Removed",
                message: "External model removed from list.",
                type: "success",
              });
            } else {
              await invoke("remove_model", { fileName: model.file_name });
            }
            await refreshModels();
          } catch (error) {
            console.error("remove_model failed", error);
            showToast({
              title: "Remove failed",
              message: String(error),
              type: "error",
            });
          }
        });
        actions.appendChild(removeBtn);
      } else {
        const button = document.createElement("button");
        button.textContent = model.downloading ? "Downloading..." : "Download";
        button.disabled = model.downloading;
        button.addEventListener("click", async (event) => {
          event.stopPropagation();
          try {
            if (!model.download_url) {
              console.error("No download URL for model", model.id);
              return;
            }
            await invoke("download_model", {
              modelId: model.id,
              downloadUrl: model.download_url,
              fileName: model.file_name,
            });
          } catch (error) {
            console.error("download_model failed", error);
            alert(`Download failed: ${error}`);
          }
        });
        actions.appendChild(button);
      }

      const progress = document.createElement("div");
      progress.className = "model-progress";
      progress.textContent = formatProgress(modelProgress.get(model.id));

      item.appendChild(header);
      item.appendChild(meta);
      item.appendChild(description);
      item.appendChild(status);
      if (model.path) {
        item.appendChild(pathLine);
      }
      item.appendChild(actions);
      item.appendChild(progress);

      container.appendChild(item);
    });
  };

  const byName = (a: ModelInfo, b: ModelInfo) =>
    (a.label || a.id).localeCompare(b.label || b.id, undefined, { sensitivity: "base" });

  const installedFiltered = activeModel
    ? installedModels.filter((model) => model.id !== activeModel?.id)
    : installedModels;
  const orderedModels = [
    ...(activeModel ? [activeModel] : []),
    ...installedFiltered.sort(byName),
    ...availableModels.sort(byName),
  ];

  renderGroup(dom.modelList, orderedModels, "No models available");
  renderHero();
}

export async function refreshModels() {
  const fetchedModels = await invoke<ModelInfo[]>("list_models");
  setModels(fetchedModels);
  renderModels();
}

export async function refreshModelsDir() {
  if (!dom.modelStoragePath) return;
  try {
    const dir = await invoke<string>("get_models_dir");
    dom.modelStoragePath.value = dir;
  } catch (error) {
    console.error("get_models_dir failed", error);
    if (settings) {
      dom.modelStoragePath.value = settings.model_storage_dir ?? "";
    }
  }
}
