# {{ name }}

A quench app: `app/` is the interface (it runs in the browser and is rendered on the server),
`core/` is logic with no interface, `server/` serves it{% if fullstack %}, and `shared/` holds the
types and the `#[server]` functions both sides use{% endif %}.

```text
quench dev      # build, serve on http://127.0.0.1:3000 and reload the browser when a file changes
quench test     # the tests, and a check that the app builds for the browser
quench build --release   # dist/: the server with the app built in, one file
quench doctor   # what this machine is missing
```
