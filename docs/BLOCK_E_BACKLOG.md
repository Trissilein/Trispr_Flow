# Block E Backlog (UX/UI Consistency + Settings IA)

Last updated: 2026-03-02

## Goal

Reduce settings complexity and improve consistency without expanding feature scope.

## Work Packages

1. Navigation and information architecture
- Unify wording across tabs/panels (Input/System/AI terms).
- Remove outdated labels and ambiguous panel descriptions.
- Ensure panel order follows user workflow.

2. Settings consistency pass
- Standardize toggle hints and range value labels.
- Normalize field spacing/alignment across panels.
- Remove duplicate controls and dead helper text.

3. Expert mode rollout foundation
- K1 complete: toggle + persistence (`trispr-expert-mode`).
- K2 pending: classify all settings into standard vs expert.
- K3 pending: add `data-expert-only` attributes and hide/show CSS.

4. Export/archive UX polish
- Verify export dialog wording and preview text consistency.
- Verify archive browser labels/count/size formatting.
- Ensure empty-state messaging is clear and actionable.

5. Accessibility and keyboard pass
- Validate focus order in Settings tab.
- Validate tab/escape behavior in dialogs.
- Ensure labels/hints remain screen-reader friendly.

## Definition of Done

- UI labels are internally consistent and no longer contradict ROADMAP naming.
- No duplicated settings controls remain in the visible UI.
- Basic keyboard navigation and focus indicators are valid in Settings and dialogs.
- `npm run build` and `npm test` stay green.
