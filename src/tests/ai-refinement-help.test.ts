import { describe, expect, it } from "vitest";
import { applyHelpTooltip, HELP_TEXTS, renderAIRefinementStaticHelp } from "../ai-refinement-help";

describe("ai-refinement-help", () => {
  it("attaches tooltip metadata to a target element", () => {
    const heading = document.createElement("h3");
    applyHelpTooltip(heading, "ollama_action_verify");

    expect(heading.dataset.helpKey).toBe("ollama_action_verify");
    expect(heading.dataset.tooltipTitle).toBe(
      HELP_TEXTS.ollama_action_verify.title
    );
    expect(heading.dataset.tooltipBody).toBe(
      HELP_TEXTS.ollama_action_verify.description
    );
    expect(heading.classList.contains("has-help-tooltip")).toBe(true);
  });

  it("attaches static help metadata without creating duplicate nodes", () => {
    document.body.innerHTML = `
      <h3 id="ai-refinement-provider-model-title" class="ai-refine-section-title">Provider & Model</h3>
      <div class="field toggle"><label class="toggle-row"><span class="field-label">Enable AI refinement</span><input id="ai-fallback-enabled" /></label></div>
      <div class="field"><span class="field-label">Provider</span><select id="ai-fallback-provider"></select></div>
      <label id="ai-fallback-model-field" class="field"><span class="field-label">Model</span><select id="ai-fallback-model"></select></label>
      <div id="ai-fallback-ollama-managed-note" class="field-hint"></div>
      <div class="field"><span class="field-label">API key</span><input id="ai-fallback-api-key-input" /></div>
      <div class="hotkey-input-group"><button id="ai-fallback-save-key">Save</button><button id="ai-fallback-clear-key">Clear</button><button id="ai-fallback-test-key">Test</button></div>
      <label class="field range"><span class="field-label">Temperature</span><input id="ai-fallback-temperature" /></label>
      <label class="field"><span class="field-label">Max tokens</span><select id="ai-fallback-max-tokens"></select></label>
      <div class="field toggle"><label class="toggle-row"><span class="field-label">Use custom prompt</span><input id="ai-fallback-custom-prompt-enabled" /></label></div>
      <div class="field"><span class="field-label">Custom prompt</span><textarea id="ai-fallback-custom-prompt"></textarea></div>
      <h3 id="ai-refinement-topic-title" class="ai-refine-section-title">Topic Detection</h3>
      <button id="topic-keywords-reset">Reset</button>
      <div id="topic-keywords-list">
        <div class="field"><span class="field-label">Productivity keywords</span><input /></div>
      </div>
    `;

    renderAIRefinementStaticHelp();
    renderAIRefinementStaticHelp();

    const providerHeading = document.getElementById("ai-refinement-provider-model-title");
    expect(providerHeading?.dataset.helpKey).toBe("ai_refinement_provider_model_section");
    expect(document.querySelectorAll("#ai-refinement-provider-model-title").length).toBe(1);

    const topicHeading = document.getElementById("ai-refinement-topic-title");
    expect(topicHeading?.dataset.helpKey).toBe("ai_refinement_topic_section");
  });
});
