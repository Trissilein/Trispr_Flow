// Audio device rendering
import { devices, outputDevices } from "./state";
import { deviceSelect, transcribeDeviceSelect } from "./dom-refs";

export function renderDevices() {
  if (!deviceSelect) return;
  deviceSelect.innerHTML = "";
  devices.forEach((device) => {
    const option = document.createElement("option");
    option.value = device.id;
    option.textContent = device.label;
    deviceSelect.appendChild(option);
  });
}

export function renderOutputDevices() {
  if (!transcribeDeviceSelect) return;
  transcribeDeviceSelect.innerHTML = "";
  outputDevices.forEach((device) => {
    const option = document.createElement("option");
    option.value = device.id;
    option.textContent = device.label;
    transcribeDeviceSelect.appendChild(option);
  });
}
