# Plan 067 — Clickable cursor affordances

Status: IN PROGRESS — Amp 2026-09-19. Issue #91; beta.11 follow-up.

Use the existing visual design unchanged. shadcn's Button documentation confirms
Tailwind v4 changed button cursors to default and recommends an application base
CSS rule. Restore pointers for enabled semantic action controls and associated
labels, preserving disabled controls and text-input cursors. Do not edit vendored
UI primitives or make non-interactive titles look clickable.

Verify actual browser computed styles for navigation, switches, labels, disabled
states and text inputs, plus frontend checks. No theme/layout changes intended.
