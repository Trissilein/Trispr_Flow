const audioPathInput = document.getElementById("audio-path");
const fileInput = document.getElementById("file-input");
const mediaPreview = document.getElementById("media-preview");
const contextInfo = document.getElementById("context-info");
const samplingEnabled = document.getElementById("sampling-enabled");
const temperature = document.getElementById("temperature");
const topP = document.getElementById("top-p");
const temperatureValue = document.getElementById("temperature-value");
const topPValue = document.getElementById("top-p-value");
const statusEl = document.getElementById("status");
const rawOutput = document.getElementById("raw-output");
const segmentsList = document.getElementById("segments-list");
const transcribeBtn = document.getElementById("transcribe-btn");
const stopBtn = document.getElementById("stop-btn");

const startupModal = document.getElementById("startup-modal");
const startupPath = document.getElementById("startup-path");
const startupAnalyzeNow = document.getElementById("startup-analyze-now");
const startupLoadOnly = document.getElementById("startup-load-only");

let activeController = null;

function setStatus(text) {
  statusEl.textContent = text;
}

function renderSegments(segments) {
  segmentsList.innerHTML = "";
  for (const segment of segments) {
    const li = document.createElement("li");
    const speaker = segment.speaker || segment.speaker_id || "speaker";
    const start = Number(segment.start_time ?? segment.start ?? 0).toFixed(2);
    const end = Number(segment.end_time ?? segment.end ?? 0).toFixed(2);
    li.textContent = `[${start}s-${end}s] ${speaker}: ${segment.text || ""}`;
    segmentsList.appendChild(li);
  }
}

function setBusy(isBusy) {
  transcribeBtn.disabled = isBusy;
  stopBtn.disabled = !isBusy;
}

async function transcribeCurrent() {
  const audioPath = audioPathInput.value.trim();
  if (!audioPath) {
    setStatus("Please provide an audio file path.");
    return;
  }

  activeController = new AbortController();
  setBusy(true);
  setStatus("Transcribing...");
  rawOutput.textContent = "";
  segmentsList.innerHTML = "";

  try {
    const response = await fetch("/api/transcribe", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        audio_path: audioPath,
        context_info: contextInfo.value || "",
        sampling_enabled: samplingEnabled.checked,
        temperature: Number(temperature.value),
        top_p: Number(topP.value),
      }),
      signal: activeController.signal,
    });

    const payload = await response.json();
    if (payload.status !== "success") {
      setStatus(`Failed: ${payload.error || "unknown error"}`);
      return;
    }

    setStatus("Completed.");
    rawOutput.textContent = JSON.stringify(payload, null, 2);
    renderSegments(payload.segments || []);
  } catch (error) {
    if (error?.name === "AbortError") {
      setStatus("Stopped by user.");
    } else {
      setStatus(`Request failed: ${String(error)}`);
    }
  } finally {
    activeController = null;
    setBusy(false);
  }
}

function stopCurrent() {
  if (!activeController) return;
  activeController.abort();
}

async function clearStartupContext() {
  try {
    await fetch("/api/clear-startup-context", { method: "POST" });
  } catch (error) {
    console.warn("clear-startup-context failed", error);
  }
}

async function loadStartupContext() {
  try {
    const response = await fetch("/api/startup-context");
    const payload = await response.json();
    const startupAudio = payload.audio_path;

    if (!startupAudio) return;

    startupPath.textContent = startupAudio;
    startupModal.classList.remove("hidden");

    startupAnalyzeNow.addEventListener("click", async () => {
      audioPathInput.value = startupAudio;
      startupModal.classList.add("hidden");
      await clearStartupContext();
      await transcribeCurrent();
    }, { once: true });

    startupLoadOnly.addEventListener("click", async () => {
      audioPathInput.value = startupAudio;
      startupModal.classList.add("hidden");
      await clearStartupContext();
      setStatus("Ready. Click Transcribe to start.");
    }, { once: true });
  } catch (error) {
    console.warn("startup-context unavailable", error);
  }
}

fileInput.addEventListener("change", () => {
  const file = fileInput.files?.[0];
  if (!file) return;
  const url = URL.createObjectURL(file);
  mediaPreview.src = url;
  if (!audioPathInput.value.trim()) {
    audioPathInput.value = file.name;
  }
});

transcribeBtn.addEventListener("click", transcribeCurrent);
stopBtn.addEventListener("click", stopCurrent);

temperature.addEventListener("input", () => {
  temperatureValue.textContent = Number(temperature.value).toFixed(1);
});

topP.addEventListener("input", () => {
  topPValue.textContent = Number(topP.value).toFixed(2);
});

setBusy(false);
setStatus("Idle.");
loadStartupContext();
