# YouTube: What Is Left

This is the current narrow list after the template, `currentScript`, and custom-element work already landed.

## Already Done

- Parsed `<template>` contents survive parsing and cloning.
- `template.content` is a real fragment wrapper with stable identity.
- `document.currentScript` is wired through the runner.
- `customElements` upgrades preserve constructor/template lookup better than before.
- The YouTube probe logs `ctor.template`, `app._template`, `app.root`, `app.shadowRoot`, and `attachShadow()` calls.

## Still Unproven

- Whether `ytd-app` is getting a usable template in a live YouTube run.
- Whether Polymer is reaching the attach/stamp phase after upgrade.
- Whether the next blocker is `attachShadow`, `_attachDom`, or fragment insertion.

## Still Missing

- A live probe run with `AURORA_DEBUG_YOUTUBE=1` captured from real YouTube.
- A focused test for the attach/stamp path if the probe shows template lookup is already fine.
- A fix for the boot-time scheduler/timer behavior if hydration starts but stalls later.
- Removal of the `customElements.define` O(document) scan if startup remains too slow.

## Next Command

```bash
AURORA_DEBUG_YOUTUBE=1 cargo run --release -- https://www.youtube.com 2>&1 | tee /tmp/aurora-youtube-probe.log
```

Then extract the relevant lines:

```bash
rg -n "\\[yt-life\\].*(probe|dom-module registered|attachShadow|define ytd-app|define ytd-masthead|upgrade ytd-app|upgrade ytd-masthead|connectedCallback ytd-app|connectedCallback ytd-masthead)" /tmp/aurora-youtube-probe.log
```

## Interpretation

- `dom-module` missing means registration/parsing/currentScript is still wrong.
- `dom-module` present but `ctor.template` is null means template lookup is still wrong.
- `ctor.template` present but no `root` or `shadowRoot` means attach/stamp is wrong.
- `root` exists but no children means fragment stamping or insertion is wrong.
- If all of that is present and it still does not hydrate, the next place to look is boot scheduling.
