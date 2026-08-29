# Manual accessibility acceptance

This record preserves the release owner's manual acceptance performed on Windows on 2026-08-29 CST. It supplements automated checks; it is not an external accessibility certification.

## Accepted candidate

- Repository state: commit `4e89e4c` (the commit itself changed documentation only; the inspected console matched the accessibility-hardened UI from the preceding private candidate).
- Browser: the BA Agent loopback console opened through the Codex desktop browser.
- Surfaces exercised: navigation, Settings, Approvals, theme and appearance switching, and the primary keyboard path.

## Checks accepted by the release owner

- Actual browser text zoom at 200%, including a second confirmation that zoom was genuinely active.
- Native Windows High Contrast behavior, including sidebar labels and active navigation.
- Windows Narrator reading order, control names, focus movement, and actionable controls on the inspected surfaces.
- Chinese labels and the three approved interface themes remained legible and operable.
- No blocking visual, keyboard, or screen-reader issue was reported in the inspected candidate.

## Current delta

Commit `36882b0` later added role-aware dispatch and configured quality-gate controls. Those controls pass desktop and Pixel 7 axe, keyboard-semantic, overflow, bilingual, theme, and interaction tests, but were not part of the 2026-08-29 manual Narrator pass. A future public release candidate should include a short manual confirmation of those two new panels before publication.
