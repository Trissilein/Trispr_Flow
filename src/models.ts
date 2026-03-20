// Model management and rendering

import { invoke } from "@tauri-apps/api/core";
import type { ModelInfo } from "./types";
import { settings, models, setModels, modelProgress, quantizeProgress } from "./state";
import * as dom from "./dom-refs";
import { getModelDescription, formatSize, formatProgress } from "./ui-helpers";
import { persistSettings } from "./settings";
import { renderHero } from "./ui-state";
import { showToast } from "./toast";

const optimizingModels = new Set<string>();
const selectedQuantByModel = new Map<string, "q5_0" | "q8_0">();

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
      let optimizeProgressElement: HTMLDivElement | null = null;
      let quantHintElement: HTMLDivElement | null = null;

      if (model.installed) {
        // Add Apply button if model is not currently active
        if (!isActive) {
          const applyBtn = document.createElement("button");
          applyBtn.className = "btn-apply-model";
          applyBtn.title = "Activate this model for transcription";
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

              // Update frontend state immediately so re-render reflects new active model
              if (settings) settings.model = model.id;

              // Success state with animation
              applyBtn.classList.remove("is-loading");
              applyBtn.classList.add("is-success");
              applyBtn.innerHTML = `
                <span class="btn-apply-icon">✓</span>
                <span class="btn-apply-text">Applied!</span>
              `;

              // Update hero card immediately
              renderHero();

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

        const lowerFileName = model.file_name.toLowerCase();
        const canOptimize =
          model.removable &&
          model.file_name.endsWith(".bin") &&
          !lowerFileName.includes("-q5_0") &&
          !lowerFileName.includes("-q8_0");

        if (canOptimize) {
          const quantizeState = quantizeProgress.get(model.file_name);
          const quantSelect = document.createElement("select");
          quantSelect.className = "model-quant-select";
          const quantChoice = selectedQuantByModel.get(model.id) ?? "q8_0";
          quantSelect.innerHTML = `
            <option value="q8_0">Q8_0 (recommended)</option>
            <option value="q5_0">Q5_0 (low VRAM)</option>
          `;
          quantSelect.value = quantChoice;
          const isOptimizing = optimizingModels.has(model.id) || Boolean(quantizeState);
          quantSelect.disabled = isOptimizing;
          quantSelect.title = "Quantization target used by Optimize (Q8 recommended; Q5 for low VRAM)";
          quantSelect.addEventListener("click", (event) => {
            event.stopPropagation();
          });
          quantSelect.addEventListener("change", (event) => {
            event.stopPropagation();
            const value = quantSelect.value === "q5_0" ? "q5_0" : "q8_0";
            selectedQuantByModel.set(model.id, value);
          });
          actions.appendChild(quantSelect);

          const optimizeBtn = document.createElement("button");
          if (isOptimizing && typeof quantizeState?.percent === "number") {
            optimizeBtn.textContent = `Optimizing ${Math.max(0, Math.min(100, Math.round(quantizeState.percent)))}%`;
          } else {
            optimizeBtn.textContent = isOptimizing ? "Optimizing..." : "Optimize";
          }
          optimizeBtn.title = "Create a quantized copy of this model (Q5 or Q8)";
          optimizeBtn.disabled = isOptimizing;
          optimizeBtn.addEventListener("click", async (event) => {
            event.stopPropagation();
            if (optimizingModels.has(model.id)) return;
            optimizingModels.add(model.id);
            quantizeProgress.set(model.file_name, {
              file_name: model.file_name,
              quant: selectedQuantByModel.get(model.id) ?? "q8_0",
              phase: "starting",
              percent: 0,
              message: "Preparing quantizer...",
            });
            renderModels();
            try {
              const quant = selectedQuantByModel.get(model.id) ?? "q8_0";
              await invoke("quantize_model", { fileName: model.file_name, quant });
              showToast({
                title: "Optimized",
                message: `Quantized model created (${quant}).`,
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
              quantizeProgress.delete(model.file_name);
              renderModels();
            }
          });
          actions.appendChild(optimizeBtn);

          if (isOptimizing) {
            const progressWrap = document.createElement("div");
            progressWrap.className = "model-quantize-progress";

            const numericPercent =
              typeof quantizeState?.percent === "number"
                ? Math.max(0, Math.min(100, Math.round(quantizeState.percent)))
                : null;
            const message = quantizeState?.message?.trim() || "Quantization in progress...";

            const info = document.createElement("div");
            info.className = "model-quantize-progress-info";
            info.textContent =
              numericPercent !== null
                ? `${numericPercent}% • ${message}`
                : message;
            progressWrap.appendChild(info);

            const bar = document.createElement("div");
            bar.className = "model-quantize-progress-bar";
            const fill = document.createElement("div");
            fill.className = "model-quantize-progress-fill";
            fill.style.width = numericPercent !== null ? `${numericPercent}%` : "28%";
            bar.appendChild(fill);
            progressWrap.appendChild(bar);
            optimizeProgressElement = progressWrap;
          }

          quantHintElement = document.createElement("div");
          quantHintElement.className = "model-quant-hint";
          quantHintElement.textContent = "Recommended: Q8_0. Use Q5_0 on low VRAM.";
        }

        const removeBtn = document.createElement("button");
        const isExternal = !model.removable;
        removeBtn.textContent = isExternal ? "Remove" : "Delete";
        removeBtn.title = isExternal ? "Remove external model from list" : "Delete model file from disk";
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
        button.title = model.downloading ? "Download in progress" : "Download this model to use for transcription";
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
      if (optimizeProgressElement) {
        item.appendChild(optimizeProgressElement);
      }
      if (quantHintElement) {
        item.appendChild(quantHintElement);
      }
      item.appendChild(progress);
      item.appendChild(actions);

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
