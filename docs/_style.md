# Documentation style guide (shared by the Quench and Forge books)

This file is for authors: Jekyll ignores files that start with `_`, so it is not published. Both books follow it.

## Two books, one theme

* `quench/docs` is **the Quench book** (framework crates). `forge/docs` is **the Forge book** (libs, CLIs, services, apps).
* Both are Jekyll sites with the `just-the-docs` remote theme (see each `_config.yml`). Navigation comes from front matter, so every page has it:

```yaml
---
title: quench-args          # the name in the sidebar
parent: Crates              # the title of the parent page (omit for a top-level page)
grand_parent: ...           # only on a third level (parent's parent)
nav_order: 3                # order among siblings
has_children: true          # only on a page that has child pages
permalink: /crates/quench-args/   # only on the section's index page
---
```

* Use **relative Markdown links to `.md` files** inside one book (`[quench-core](../quench-core/index.md)`); the `jekyll-relative-links` plugin turns them into site links, and they also work on GitHub. Links **between** the two books are not possible with relative paths on a published site: write "see `quench-http` in the Quench book" with the name in code font, and no link.
* One `#` heading per page, equal to the front-matter title is **not** required: the theme renders the title; start page bodies at `##` after one short lead paragraph (no `#` heading in the body).
* Code blocks always carry a language (`rust`, `toml`, `yaml`, `bash`, `text`, `json`, `http`).

## Exactness rules (the point of the exercise)

1. **Every statement is checked against the source** in this repository (the code, `Cargo.toml`, tests, `--help` output of a built local binary). Never copy from the old docs without checking them. Where the old text and the code disagree, the code wins and the disagreement goes into your report (see "Reports").
2. Signatures, field names, defaults, env var names, flag names, route paths, status codes and error texts are copied from the code, not paraphrased. Say "not implemented" or "unspecified" rather than guess; never invent a behaviour.
3. Examples are taken from, or checked against, the crate's tests and `examples/` where they exist. An example that cannot be run in the book must still compile in principle: use real paths and real names. Mark anything you could not verify with a visible *Unverified:* line.
4. State the crate **version** (from its `Cargo.toml`) on the overview page and keep `[features]`, optional dependencies and the dependency policy (no third-party crates beyond the allowed set) exact.
5. No marketing language, no filler. Short sentences, present tense, concrete.

## Libraries and framework crates: "crates.io sort of"

One directory per crate: `docs/crates/<crate>/` (Quench) or `docs/libs/<lib>/` (Forge). Pages (add more when the crate needs them; every page has front matter with `parent` = the crate's `index.md` title):

| File | Content |
|---|---|
| `index.md` | What it is, why it exists (what it replaces), version, MSRV/edition, install snippet (`Cargo.toml`), feature flags table, the dependency list with why, a "Where to go next" list, a minimal complete example. `has_children: true`, `parent: Crates` (or `Libraries`). |
| `usage.md` | Task-oriented guide: 4-10 worked examples from the common to the advanced, each with the goal, code, the output or effect, and the pitfalls. |
| `api.md` (or `api-<module>.md` when long) | **Reference of the public API**, module by module: every `pub` item (types, traits, functions, macros, constants, re-exports) with its exact signature, a description of what it does, parameters and return values, errors, panics, thread-safety/`Send`/`Sync` notes where relevant, and a short example for the non-trivial ones. Group by module in the same order as the source; mention `#[must_use]`, feature gates, and `unsafe` (there should be none). |
| `configuration.md` | Env vars, config files, builder options, defaults and valid ranges (only if the crate has any). |
| `errors.md` | Error types and when each occurs (only if non-trivial). |
| `internals.md` | Design notes that matter to users (algorithms, limits, performance traits, security properties, wire/file formats with byte-exact layouts) (only if there is something to say). |
| `testing.md` | How the crate is tested and how users test code built on it (only if relevant). |

Proc-macro crates (`*-macros`) are documented in their own directory too: every attribute/derive, its arguments, what it generates, and compile-error cases.

## Services: API in OpenAPI form

One directory per service: `docs/services/<service>/` in the Forge book.

| File | Content |
|---|---|
| `index.md` | Purpose, crates in the service (`core`, `app`, `web`, `service`), ports/paths (`BASE_PATH`), dependencies (Postgres, Redis, other services), authentication model, links to the other pages. `has_children: true`, `parent: Services`. |
| `api.md` | The HTTP API, **rendered from `openapi.yaml`**: one section per tag, one subsection per operation: method and path, summary, auth/permission needed, path/query/header parameters, request body (field table, JSON example), responses (status, body, example), errors. Keep it in step with the YAML. |
| `openapi.yaml` | **OpenAPI 3.1** document, hand-checked against the code, in the service's docs directory (published as a static file by Jekyll: give it front matter-free content and add `/services/<service>/openapi.yaml` link on the api page). Contains: `info` (title, version = the service's crate version, description), `servers` (`/{base}` variable), `tags`, `paths` for **every** route registered by the service (including `/ui/...` SSO routes and static asset routes, tagged `ui`), `components.schemas` for every request/response type (taken from the `*-core` and service types and their `serde` attributes, with required/optional exactly as serialized), `components.securitySchemes` (session cookie, bearer app/access token), per-operation `security`, and `x-permission: service:action` where the code enforces a grant. Reuse schemas with `$ref`; give examples. |
| `configuration.md` | Every env var the service reads (name, type, default, meaning, where read), config files, feature flags (`FEATURE_*`), secrets (names only, never values), ports. |
| `data.md` | Database schema it owns (tables, columns, constraints, migrations in order), Redis keys, files. |
| `operations.md` | Run locally (foreman), run in the cluster (image, env, probes, `homecloud` overlay names), health endpoints, metrics, logging, failure modes, limits. |
| `client.md` | The browser client (`app`/`web` crates) for a service with a UI: screens and routes, requests/messages between app and host, i18n keys and locales, styling hooks. For mobile/desktop apps: screens, host, platform specifics. |
| other | `events.md` (SSE streams), `permissions.md`, `scheduler.md`, etc. when the service has them. |

`anvil docs check` (see the anvil pages in the Forge book; configured in `forge/.anvil/docs.toml`) verifies that **every route macro in a service's source (`#[get("...")]`, `#[post(...)]`, ... ) appears as a path+method in its `openapi.yaml`**, and that the YAML parses and has no dangling `$ref`. Run it before reporting. Fix the docs, not the script.

## CLIs

One directory per CLI: `docs/cli/<tool>/`.

| File | Content |
|---|---|
| `index.md` | Purpose, install (`cargo install`/`anvil install`), global options, configuration files, environment variables, exit codes, concepts. `has_children: true`, `parent: Command-line tools`. |
| `commands.md` (or one page per command group) | **Every** command and subcommand: synopsis, every flag/argument (type, default, env fallback, required), behaviour, side effects, exit codes, output format, and at least one real example with sample output. Include aliases and hidden-but-working forms. |
| `repl.md` | REPL / interactive modes if any: how to enter, prompt, every in-session command, completion, history, quit. |
| `configuration.md` | Every config file and key it reads (schema, defaults, merge rules, examples). |
| `recipes.md` | End-to-end workflows combining commands. |

Get the command list from the argument definitions in the source **and** from the built binary's `--help` for every subcommand (build and run locally; commands that touch real services, credentials or the network must not be run - read their source instead).

## Reports (gaps and mismatches)

Every author writes `docs/_reports/<area>.md` (ignored by Jekyll) with three lists: **(1) stale or wrong statements found in the old docs** (what they said, what the code does), **(2) things that exist in the code but were undocumented**, **(3) open questions** (behaviour you could not determine from the code, TODOs and suspected bugs, with file:line). Do not fix source code to match the docs; list suspected bugs.

## Mechanics and limits

* Replace the old flat page of a crate/service/CLI with the new directory, and fix links that pointed at the old page (READMEs of the crates, the other books' pages). Delete the old file only when its content is fully covered.
* Do not change anything under `src/`, `tests/`, manifests or lockfiles: documentation only (plus the check script).
* Never commit, push, publish, `git init`, or run git commands that change anything. Never read `.env`, `homecloud/.env`, `.anvil/secret.toml` or other secrets. Never contact real services (warehouse, gatehouse, the cluster, `ennor.ddns.net`); run only local binaries, and never `pkill -f`. Keep each shell call under two minutes (background + poll for builds).
* The pages are long on purpose: completeness over brevity, but no padding. A crate with 40 public items has 40 documented items.
