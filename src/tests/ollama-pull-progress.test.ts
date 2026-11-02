import { describe, expect, it } from "vitest";
import { PullProgressTracker } from "../ollama-pull-progress";

const A = "sha256:4c27e0f5b5ad0000000000000000000000000000000000000000000000000000";
const B = "sha256:8b1d2e3f4a5b0000000000000000000000000000000000000000000000000000";
const C = "sha256:c0ffee0000000000000000000000000000000000000000000000000000000000";

// Shapes taken from the real `/api/pull` NDJSON stream: `completed` is missing on the first line of a
// layer and on tiny layers, lines of different layers alternate, the tail has no digest at all.
describe("PullProgressTracker", () => {
  it("has no percentage until a layer size is known", () => {
    const tracker = new PullProgressTracker();
    tracker.record({ status: "pulling manifest" });
    expect(tracker.snapshot()).toMatchObject({ percent: null, status: "pulling manifest", total: 0 });
  });

  it("does not jump back to 0 when the stream switches to another layer", () => {
    const tracker = new PullProgressTracker();
    tracker.record({ status: "pulling manifest" });
    tracker.record({ status: "pulling 4c27e0f5b5ad", digest: A, total: 1000 });
    tracker.record({ status: "pulling 4c27e0f5b5ad", digest: A, total: 1000, completed: 600 });
    expect(tracker.snapshot().percent).toBe(60);

    // Next layer announces itself without `completed`: the old single-line bar showed 0 here.
    tracker.record({ status: "pulling 8b1d2e3f4a5b", digest: B, total: 1000 });
    const afterSwitch = tracker.snapshot();
    expect(afterSwitch.completed).toBe(600);
    expect(afterSwitch.total).toBe(2000);
    expect(afterSwitch.percent).toBe(30);

    tracker.record({ status: "pulling 4c27e0f5b5ad", digest: A, total: 1000, completed: 1000 });
    tracker.record({ status: "pulling 8b1d2e3f4a5b", digest: B, total: 1000, completed: 500 });
    expect(tracker.snapshot().percent).toBe(75);
  });

  it("never moves backwards when a part restarts and reports less", () => {
    const tracker = new PullProgressTracker();
    tracker.record({ status: "pulling a", digest: A, total: 1000, completed: 800 });
    tracker.record({ status: "pulling a", digest: A, total: 1000, completed: 300 });
    expect(tracker.snapshot().completed).toBe(800);
  });

  it("clamps a layer's completed bytes to its total", () => {
    const tracker = new PullProgressTracker();
    tracker.record({ status: "pulling a", digest: A, total: 1000, completed: 5000 });
    expect(tracker.snapshot()).toMatchObject({ completed: 1000, percent: 100 });
  });

  it("ignores lines without digest or size (they only change the status text)", () => {
    const tracker = new PullProgressTracker();
    tracker.record({ status: "pulling a", digest: A, total: 1000, completed: 100 });
    tracker.record({ status: "something else" });
    tracker.record({ status: "pulling zero", digest: B, total: 0 });
    tracker.record({ status: "pulling nan", digest: C });
    expect(tracker.snapshot()).toMatchObject({ completed: 100, total: 1000, status: "pulling nan" });
  });

  it("uses the expected total so the bar does not fall when later layers appear", () => {
    const tracker = new PullProgressTracker(6_580);
    tracker.record({ status: "pulling a", digest: A, total: 5_490, completed: 5_490 });
    // Layer A alone would be 100 %, but 1 090 more bytes are still to come.
    expect(tracker.snapshot().percent).toBe(83);
    tracker.record({ status: "pulling b", digest: B, total: 990 });
    tracker.record({ status: "pulling c", digest: C, total: 100 });
    expect(tracker.snapshot().percent).toBe(83);
  });

  it("an expected total smaller than the real layers never inflates the percentage", () => {
    const tracker = new PullProgressTracker(100);
    tracker.record({ status: "pulling a", digest: A, total: 2000, completed: 1000 });
    expect(tracker.snapshot()).toMatchObject({ total: 2000, percent: 50 });
  });

  it("small layers that never reported `completed` count as done once verification starts", () => {
    const tracker = new PullProgressTracker(2_100);
    tracker.record({ status: "pulling a", digest: A, total: 2_000, completed: 2_000 });
    tracker.record({ status: "pulling tiny", digest: B, total: 100 });
    expect(tracker.snapshot().percent).toBe(95);

    tracker.record({ status: "verifying sha256 digest" });
    expect(tracker.snapshot()).toMatchObject({ percent: 100, finalizing: true, completed: 2_100, total: 2_100 });
    tracker.record({ status: "writing manifest" });
    tracker.record({ status: "success" });
    expect(tracker.snapshot().percent).toBe(100);
  });

  it("a fully cached model (no layer lines) does not invent a percentage", () => {
    const tracker = new PullProgressTracker();
    tracker.record({ status: "pulling manifest" });
    tracker.record({ status: "verifying sha256 digest" });
    expect(tracker.snapshot().percent).toBeNull();
  });
});
