---
title: Internals
parent: quench-k8s
grand_parent: Crates
nav_order: 5
---

## Requests

Every request is `{config.server}{path}` with `Accept: application/json`. Query parameters are added by `quench-client`'s `query`. Request bodies are JSON (`quench_serde::json::to_vec`), with `Content-Type` `application/json` except for patches (see [API](api.md)).

A non-2xx response becomes `Error::Api(Status)`; the body is read as text and parsed per [Errors](errors.md). Transport failures become `Error::Transport(<message>)`.

## Pagination

`Api::list` loops: it sends `limit=<page_size>` when set and `continue=<token>` after the first page, appends `items`, and stops when `metadata.continue` is missing or empty. The returned `resource_version` is that of the last page.

## Watch

The request is a normal GET of the collection with `watch=true&allowWatchBookmarks=true[&resourceVersion=..]`, read as a stream and split into lines with `lines`. Each line is a JSON object `{"type": .., "object": ..}`:

| `type` | Result |
|---|---|
| `ADDED`, `MODIFIED`, `DELETED`, `BOOKMARK` | the matching `WatchEvent` with `object` |
| `ERROR` | `WatchEvent::Error(Status)` built from `object` (its `code` field, else 0, and its `Status` fields) |
| other | `Err(Error::Decode("unknown watch event type ..."))` |
| no `object` | `Err(Error::Decode("a watch event without an object"))` |

A line that is not JSON is `Err(Error::Decode(..))`. The stream ends when the server closes the response; the caller restarts it from the last `resourceVersion`. The crate does not retry or re-list by itself.

## Discovery

`Discovery::run` fetches `/apis` for the group list, then in parallel (`quench_core::future::join_all`) `/api/v1` and `/apis/<group>/<preferredVersion>` for every group that has a name and a preferred version. A group whose request fails (for example because the account may not read it) is silently left out. A failure of the `/apis` request itself is returned as the error. From each resource list, entries without `name` or `kind` and subresources (name containing `/`) are skipped; `namespaced` must be exactly `true` for `Scope::Namespaced`. The core group is always assumed at `v1`.

## Delete body

Only sent when `propagation` or `grace_period_seconds` is set; `DeleteParams::dry_run` alone sends no body.

## Credentials

`Auth::BearerFile` is read with `std::fs::read_to_string` (blocking) on every request. `Debug` output of `Auth` and `Config` never contains credentials. `ClientCertificate` is applied once, when the HTTP client is built.

## Not implemented

Exec and attach (WebSocket upgrades), port forwarding, reflectors/informers, credential plugins, typed models, kubeconfig merging, request retries.
