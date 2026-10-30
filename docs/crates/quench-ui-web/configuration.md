---
title: Configuration
parent: quench-ui-web
grand_parent: Crates
nav_order: 7
---

`quench-ui-web` has no environment variables or config files. What can be configured is how the page loads `quench.js`, and what `quench.js` reads from the page.

## Loading `quench.js`

```html
<div id="app"></div>
<script type="module" src="/quench.js?wasm=/app.wasm"></script>
```

`quench.js` reads the query of its own URL (`import.meta.url`):

| Parameter | Meaning |
|---|---|
| `wasm=<url>` | The module to load. If present, `start(url)` runs at once. Reading the parameter from the script URL means no inline script (so no CSP nonce) is needed. |
| `dev=1` / `dev=0` | Forces development mode on or off. Any other value counts as off when the parameter is present. |

Without `wasm=` nothing starts automatically; the module exports `start`:

```js
export async function start(url, options = {})
```

`start` returns `{ exports, handles }` (`exports`: the instance exports; `handles()`: how many nodes are in the handle table). In development mode it also sets `window.__quench` to that object. A boot failure is logged as `quench: could not start:` on the console.

## Development mode (`dev`)

Precedence: `options.dev`, then (for the auto-start) `?dev=`, then `window.QUENCH_DEV` if it is a boolean, then true when `location.hostname` is `localhost`, `127.0.0.1` or `[::1]`. In development mode:

* a panic (`q_log` level 3) and a trap in the module show a full-page overlay (`<pre>`, `z-index: 2147483647`);
* each hydration mismatch is logged to the console with the path of the parent;
* `window.__quench` is set.

## What the page provides

| Element / API | Use |
|---|---|
| `#app` | the root container (handle 1); `document.body` if absent |
| `<script type="application/json" id="__q_state">` | state for `initial_state()` |
| `localStorage` keys starting `q:` | read at start into `browser::storage_get` |
| `Intl.DateTimeFormat().resolvedOptions().timeZone` | `browser::time_zone()`, `UTC` if empty |
| `navigator.userAgent` containing `QuenchHost/` | marks a host app (bridge enabled) |

## Content Security Policy

The policy must allow WebAssembly: `'wasm-unsafe-eval'` in `script-src`. Styles are applied through the style object (`CSSStyleDeclaration.setProperty`), which a strict `style-src` allows; `Element::style(..)` of `quench-ui` writes a `style` attribute, which a policy without `'unsafe-inline'` in `style-src-attr` blocks. *Unverified:* the policy behaviour was not re-tested here; it is from the code comments and the old page.
