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
  outputDevices.forEach((device) => {
    const option = document.createElement("option");
    option.value = device.id;
    option.textContent = device.label;
    select.appendChild(option);
  });
}
