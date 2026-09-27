# Plan 067 — Clickable cursor affordances

Status: VERIFIED — Amp 2026-09-19. Issue #91; beta.11 follow-up.

Use the existing visual design unchanged. shadcn's Button documentation confirms
Tailwind v4 changed button cursors to default and recommends an application base
CSS rule. Restore pointers for enabled semantic action controls and associated
labels, preserving disabled controls and text-input cursors. Do not edit vendored
UI primitives or make non-interactive titles look clickable.

Verify actual browser computed styles for navigation, switches, labels, disabled
states and text inputs, plus frontend checks. No theme/layout changes intended.

Implemented in app-level base CSS only. Browser verification rendered the real
Button, SidebarMenuButton, Switch, FieldLabel and Input components. Ten computed
cursor states passed: enabled actions use pointer; disabled controls/labels do
not; text entry remains text; inert headings remain auto. Label click toggled the
enabled switch. This caught a disabled-label specificity bug before completion.
Typecheck, lint, production build and 732 scoped frontend tests passed. No native
code changed. Reference: https://ui.shadcn.com/docs/components/base/button#cursor .
