// Audio device rendering
import { devices, outputDevices } from "./state";
import { deviceSelect, transcribeDeviceSelect } from "./dom-refs";

export function renderDevices() {
  if (!deviceSelect) return;
  const select = deviceSelect;
  select.innerHTML = "";
  devices.forEach((device) => {
    const option = document.createElement("option");
    option.value = device.id;
    option.textContent = device.label;
    select.appendChild(option);
  });
}

export function renderOutputDevices() {
  if (!transcribeDeviceSelect) return;
  const select = transcribeDeviceSelect;
  select.innerHTML = "";

  // Always offer the Windows default as the first, explicitly selectable option.
  // Without this, setting select.value = "default" silently fails and the dropdown
  // snaps to the first real device while settings.transcribe_output_device stays "default".
  const defaultOption = document.createElement("option");
  defaultOption.value = "default";
  defaultOption.textContent = "Default (System)";
  select.appendChild(defaultOption);

  outputDevices.forEach((device) => {
    const option = document.createElement("option");
    option.value = device.id;
    option.textContent = device.label;
    select.appendChild(option);
  });
}
