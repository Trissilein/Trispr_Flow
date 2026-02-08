// Model management and rendering

import { invoke } from "@tauri-apps/api/core";
import type { ModelInfo } from "./types";
import { settings, models, setModels, modelProgress } from "./state";
import * as dom from "./dom-refs";
import { getModelDescription, formatSize, formatProgress } from "./ui-helpers";
import { persistSettings } from "./settings";
import { renderHero } from "./ui-state";
import { showToast } from "./toast";

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
      if (model.installed) {
        item.classList.add("selectable");
        item.addEventListener("click", async () => {
          if (!settings) return;
          settings.model = model.id;
          await persistSettings();
          renderModels();
        });
      }

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
