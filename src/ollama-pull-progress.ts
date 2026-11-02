import type { OllamaPullProgress } from "./types";

type Layer = { total: number; completed: number };

export type PullSnapshot = {
  /** 0-100, or null while no layer size is known yet (only "pulling manifest"). */
  percent: number | null;
  completed: number;
  total: number;
  /** Status text of the newest line, e.g. "pulling 8d3f12a9c04b". */
  status: string;
  finalizing: boolean;
};

/** Ollama is done downloading when it starts verifying or writing the manifest. */
function isFinalizingStatus(status: string): boolean {
  return /^(verifying|writing manifest|removing|success)/i.test(status.trim());
}

/**
 * Folds the NDJSON lines of one `/api/pull` into a single progress for the whole model.
 *
 * Every line describes one layer (`digest`, `total`, `completed`); `completed` is missing on the
 * first line of a layer and for tiny layers, and lines of different layers alternate. Looking at
 * only the newest line made the bar jump to 0 whenever the layer changed.
 */
export class PullProgressTracker {
  private readonly layers = new Map<string, Layer>();
  private status = "";
  private finalizing = false;

  constructor(private readonly expectedTotalBytes = 0) {}

  record(line: Pick<OllamaPullProgress, "status" | "digest" | "total" | "completed">): void {
    const status = (line.status ?? "").trim();
    if (status) this.status = status;
    if (isFinalizingStatus(status)) {
      this.finalizing = true;
      for (const layer of this.layers.values()) layer.completed = layer.total;
      return;
    }
    const digest = line.digest?.trim();
    const total = line.total;
    if (!digest || typeof total !== "number" || !(total > 0)) return;
    const layer = this.layers.get(digest) ?? { total, completed: 0 };
    layer.total = total;
    if (typeof line.completed === "number" && line.completed >= 0) {
      // Never step back: a restarted part may report a smaller number for a moment.
      layer.completed = Math.max(layer.completed, Math.min(line.completed, total));
    }
    this.layers.set(digest, layer);
  }

  snapshot(): PullSnapshot {
    let layerTotal = 0;
    let completed = 0;
    for (const layer of this.layers.values()) {
      layerTotal += layer.total;
      completed += layer.completed;
    }
    // Layers show up one by one; the catalog size keeps the bar from dropping when the next one appears.
    const total = this.finalizing ? layerTotal : Math.max(layerTotal, this.expectedTotalBytes);
    const percent =
      this.finalizing && this.layers.size > 0
        ? 100
        : total > 0
          ? Math.min(100, Math.floor((completed / total) * 100))
          : null;
    return { percent, completed, total, status: this.status, finalizing: this.finalizing };
  }
}
