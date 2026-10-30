---
title: Internals
parent: quench-template
grand_parent: Crates
nav_order: 6
---
{% raw %}
## Pipeline

1. `split` cuts the source into text, `{{ }}` and `{% %}` pieces, applying `-` whitespace control and `raw`, and drops one trailing newline when the last piece is text. Comments are discarded here. String literals are skipped when looking for a tag's end.
2. `tokenize` turns tag content into identifiers, integers (`i64`), floats, strings and the operators `== != <= >= // ** + - * / % ~ < > = ( ) [ ] { } , : . |`. Unknown characters are `` unexpected `c` ``.
3. A recursive-descent parser builds `Expr` and `Stmt` trees. `if` and `for` bodies are collected by looking for their closing tags.
4. `Runner` interprets the tree. Values during evaluation are `Val`: undefined, data (`Value`), a macro or an imported module.

## Scopes

* A runner holds a stack of scopes (`Rc<RefCell<HashMap>>`). Lookup goes from the innermost scope outward, then the render context's top-level map, then the globals.
* `for`, `with`, macro calls and `include` push a scope; `set` writes the innermost scope.
* A macro captures a handle on the top scope of its defining template and, when called, runs with exactly that scope plus a fresh scope for its parameters. Because the scope is shared, a macro sees later `set`s of the template.
* `import` runs the imported template once per `import` statement with a null context, discards its output and exposes the top scope as a module. Nothing is cached between imports, so the imported template is re-run each time it is imported in a render.
* `include` runs with a clone of the scope handles and a fresh innermost scope.
* Depth is limited to 32 for macros, imports and includes.

## Undefined values

A missing name is `Val::Undefined`. Printing it prints nothing; as a condition it is false; iterating gives an empty list. `x.attr` on undefined is an `Undefined` error; `x[i]` on undefined is the same. `default` and `is defined`/`undefined` catch those errors and treat the value as undefined. `x.attr` where `x` is not a map (none, number, list, string) gives undefined, not an error. A missing key of a map is undefined. A missing macro parameter is undefined.

## Numbers

Integer arithmetic runs on `i128` and is range-checked back to `i64`/`u64` (`integer overflow` beyond). `/` and `**` with a negative exponent produce floats; `//` and `%` follow Python's floor semantics. Booleans count as 0/1 in arithmetic and comparison. Floats print with at least one fractional digit when whole and below 1e16.

## Cost

Everything is cloned liberally (`items()` clones lists; `slice` copies). The engine is meant for configuration and page templates, not for large data sets. No template compilation cache beyond the parsed tree held by the environment.

## Differences from Jinja2/minijinja that matter

* No inheritance (`extends`/`block`), `call`, `filter` blocks, `trim_blocks`, autoescaping, line statements or custom delimiters.
* `true`/`false`/`none` print in lower case.
* `-2 ** 2` is `4`.
* `loop` has no `cycle`/`changed`/`depth`.
* `escape`/`e`/`safe`/`forceescape` do nothing.
* `round` always returns a float.
{% endraw %}
