# Sage: the migration checklist

**Where it lives:** `forge/services/sage`, the pattern of [PATTERN.md](PATTERN.md) (Gauge, Workbench) with the one thing they lacked: an answer that streams in. Conveyor's live log ([conveyor.md](conveyor.md)) is the model for that: `quench_ui_web::EventSource`, named events, one `done` that ends the stream.

```text
service/   sage-service: every API (kept), the SSO routes, the page that starts the client; the htmx pages, forge-ui and quench-web are gone
core/      sage-core: the wire model, the stream's events, the markdown subset, drafts, ids: no I/O, builds for the browser
app/       sage-app: home (chat), initializing, the sidebar, the dialogs over `Request`/`Msg`
web/       sage-web: the wasm module: `fetch` (JSON) + `EventSource` (the answer) with the session cookie
```

## Behaviour to match (read from the htmx pages)

- **Home** (`/ui/home?conversation_id=&project_id=`): redirects to the initializing screen until every default model runs (or switchboard is down).
  - Chat history of the active branch: user and Sage bubbles; empty conversation shows the welcome message.
  - Message **navigator**: one dot per message with a tooltip (first 30 characters), click scrolls to the message, the last/clicked one is active.
  - **Composer**: auto-growing textarea (Enter sends, Shift+Enter breaks the line), send button; disabled with a warning when no chat model; search-provider select (default marked), model select (chat-capable instances only; "No models available" / "Switchboard unavailable"); paperclip to attach files (staged chips with name, size, status badge that updates queued -> processing -> ready/failed, retry on failed, remove; image thumbnails); the accept list comes from the server.
  - **Sending**: the user bubble (with attachment chips, Edit) and a **"Sage is thinking..."** placeholder appear at once; the answer streams in, status lines for the grounded pipeline (planning, searching with the queries, reading, checking), then the final answer with Sources and controls; an error replaces the placeholder with "Error: ...".
  - **Branches**: previous/next between alternative answers (`n/total`), **Regenerate** on the last answer ("Sage is regenerating..."), **Edit** on the last user message (Save & Submit makes a sibling user message and Sage answers it).
  - Answers are **markdown**: code fences with a language header and a Copy button, headings, lists, tables, bold/italic/inline code/links, horizontal rules, and the embedded tool-result cards (`web_search "..."`).
  - Sources block under answers (file excerpts with similarity, web pages `[n] domain`).
  - A conversation whose last message is the user's (an edit, a failed turn) is answered automatically on opening.
- **Sidebar**: New Chat; **Projects** (collapsible, "New" opens the create dialog; each project links to `?project_id=`; the open project shows a collapsible **Files** list: download link, status badge, delete with confirmation; its conversations indented below); **History** (conversations with no project); each conversation has a menu with Delete (confirmation dialog); blank titles show "New chat"; the title is the first 30 characters of the first message.
- **Initializing** (`/ui/initializing`): one row per default model with state (running / starting / queued / failed / connecting), an embedding tag, "waiting for the model service" warning; polled every 3 s and goes to home when all run.
- Five locales (`i18n/*.ftl`, shared with the old pages), light/dark/palette follow the shell.
- Auth unchanged: `/ui/login`, `/ui/auth/callback`, `/ui/logout`, `/ui/status`, `/ui/refresh`; `/api/v1/chat*` (the machine API: `POST /api/v1/chat`, capabilities, metrics, costs, context status) and `/api/v1/files*` keep their shapes.

## Choices

- **Markdown:** rendered **in the client** from the message text (`sage_core::markdown`, tested). The old server-side `format_message` escaped everything except the tool-result cards it embeds as raw HTML in the stored answer; the client parses those cards (`<div class="tool-result ...">`) into structured blocks and shows their text, so no HTML is ever injected. Same subset as before (fences, headings, lists, tables, hr, bold, italic, inline code, links); a link is only clickable when its URL is http(s)/mailto.
- **Streaming:** `GET /api/v1/turns/{id}/stream` is a plain `EventSource` stream (no POST body). Named events, data is JSON (no HTML): `status` `{stage,detail}`, `delta` `{text}` (the chunk), `replace` `{text}` (verification rewrote the answer), `done` `{conversation_id,user_message_id,message_id,title}`, `failure` `{message}` (`error` is the browser's own connection event). The turn is started by a JSON `POST /api/v1/turns` (or `/regenerate`, `/conversations/{id}/resume`) that stores the pending request and returns its id. After `done` the client re-reads the conversation and the sidebar (the stored answer may differ from the streamed one).
- **Uploads:** the old `<input type=file>` posted multipart through htmx. `fetch` from wasm cannot read a chosen file, so `forge-shell/assets/shell.js` gets a small generic hook: an `<input type=file data-upload-url data-upload-fields data-upload-result>` posts each chosen file as `multipart/form-data` (the server's existing format) and hands `{name,status,body}` to the hidden input named by `data-upload-result` through its `input` event. `data-scroll-to` (click scrolls an element into view) is the second hook (navigator dots). Both tested in `forge-shell`.
- Ids of fresh conversations come from `GET /api/v1/conversation-id` (nothing stored); the first message or attachment creates the row.

## What the API lacks for a client

Added to `/api/v1` (behind the same `Auth`, owner-scoped like the rest):
- `GET /api/v1/models` (chat instances, default-model states, search providers, accept list; the home gate and the initializing screen), `GET /api/v1/conversation-id`.
- `GET /api/v1/conversations`, `GET /api/v1/conversations/{id}` (active branch with siblings, sources, attachments), `DELETE`, `POST .../switch`, `POST .../resume`.
- `GET /api/v1/projects`, `POST /api/v1/projects`.
- `POST /api/v1/turns`, `POST /api/v1/turns/regenerate`, `GET /api/v1/turns/{id}/stream`, `POST /api/v1/messages/{id}/edit`.
- `POST /api/v1/files/staged` (multipart; creates the conversation row when it is not stored yet, like the old attach).
- Kept as they were: `/api/v1/files*` (list, get, download, reprocess, chunks, delete), `/ui/chat/stats/{id}`.

## Checklist

### Phase 1: the service API
- [x] 1.1 Wire model in `sage-core`; `models`, `conversation-id`, `conversations`, `projects` routes with tests.
- [x] 1.2 Turns: send, regenerate, resume, edit, switch; the stream as plain SSE; tests adapted from the old stream tests.
- [x] 1.3 `files/staged` with tests.

### Phase 2: `sage-core`
- [x] 2.1 Wire model, stream events, drafts (message, project name), preview/title rules, file status.
- [x] 2.2 Markdown subset + tool-result cards.

### Phase 3: `sage-app`
- [x] 3.1 Shell: top bar, language, sidebar (new chat, projects, files, history, menus), banner.
- [x] 3.2 Home/chat: history, navigator, composer, models/providers, attachments.
- [x] 3.3 Streaming turn: placeholder, status, deltas, done/failure, regenerate, edit, branches, auto-resume.
- [x] 3.4 Dialogs: delete conversation, delete file, new project; initializing screen.
- [x] 3.5 Screen tests with a fake stream.

### Phase 4: `sage-web` and the service
- [x] 4.1 `fetch` for every request, `EventSource` for the turn, uploads through the shell hook.
- [x] 4.2 The service serves the client at `/ui/home`, `/ui/initializing`; htmx pages, `forge-ui`, `quench-web` leave the crate.
- [x] 4.3 `.anvil/docker.toml`, `web/.gitkeep`, `.gitignore`, icon.
- [x] 4.4 Run locally against a stand-in model server and click through it.

## Result

Verified in Chrome against the real `sage-service`, a throwaway pgvector Postgres and a local Python stand-in for switchboard/gatehouse/vLLM: a new chat, the answer arriving token by token (code block, list, table, bold) with the "Sage is thinking" placeholder, the stored conversation re-read at the end (address gets `conversation_id`), regenerate (2/2), a file chosen through the shell hook (chip queued -> ready), a new project, a conversation inside it, delete with confirmation, language switch (German), theme switch, and the error state with the model service down (the error bubble, then the initializing screen with "connecting").

Not verified in a browser: edit and branch arrows, the grounded-pipeline status lines (screen tests with a fake stream cover them), image thumbnails, the project Files list with real files, the image build's wasm step, a real gatehouse login.
