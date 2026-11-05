// Audio device rendering
import { devices } from "./state";
import { deviceSelect } from "./dom-refs";

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
