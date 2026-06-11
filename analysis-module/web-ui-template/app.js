const startButton = document.getElementById("start-analysis");
const audioInput = document.getElementById("audio-input");
const jobState = document.getElementById("job-state");
const timeline = document.getElementById("timeline");
const segmentsList = document.getElementById("segments-list");

const demoSegments = [
  { speaker: "SPEAKER_00", start: 0.0, end: 5.2, text: "Welcome to the analysis module." },
  { speaker: "SPEAKER_01", start: 5.2, end: 11.1, text: "This is a placeholder web template." },
  { speaker: "SPEAKER_00", start: 11.1, end: 16.8, text: "Wire this to the real API in Phase 1." }
];

function renderDemoResult() {
  timeline.innerHTML = demoSegments
    .map((segment) => `<div>[${segment.start.toFixed(1)}s - ${segment.end.toFixed(1)}s] ${segment.speaker}</div>`)
    .join("");

  segmentsList.innerHTML = demoSegments
    .map(
      (segment) =>
        `<li class="segment-item"><strong>${segment.speaker}</strong><br /><small>${segment.start.toFixed(1)}s - ${segment.end.toFixed(1)}s</small><p>${segment.text}</p></li>`
    )
    .join("");
}

startButton?.addEventListener("click", () => {
  const file = audioInput?.files?.[0];
  if (!file) {
    jobState.textContent = "Select an audio file first.";
    return;
  }

  jobState.textContent = `Mock job queued for ${file.name}.`;
  window.setTimeout(() => {
    jobState.textContent = `Mock job completed for ${file.name}.`;
    renderDemoResult();
  }, 400);
});

document.querySelectorAll("button[data-format]").forEach((button) => {
  button.addEventListener("click", () => {
    const format = button.getAttribute("data-format");
    jobState.textContent = `Export placeholder: ${format?.toUpperCase()}.`;
  });
});
