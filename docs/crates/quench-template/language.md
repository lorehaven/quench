---
title: Language reference
parent: quench-template
grand_parent: Crates
nav_order: 5
---
{% assign lt = '{%' %}{% raw %}
Everything on this page is read from `src/syntax.rs` and `src/render.rs`. Output shown after `=>` was produced by rendering the template against the context given.

## Delimiters

| Delimiter | Meaning |
|---|---|
| `{{ expr }}` | print the value of the expression |
| `{% tag ... %}` | a statement |
| `{# ... #}` | a comment (may contain anything but `#}`; emits nothing) |

String literals inside `{{ }}` and `{% %}` are skipped when looking for the closing delimiter, so `{{ "}}" }}` is fine. A tag that is never closed is `` unclosed `{{` ``/`{%`/`{#`.

### Whitespace control

* `{{-`, `{%-`, `{#-` strip **all** whitespace (spaces and newlines) before the tag. `-}}`, `-%}`, `-#}` strip all whitespace after it.
* `{%+` is accepted and means a plain `{%` (there is no `lstrip_blocks` to turn off).
* There is no `trim_blocks`/`lstrip_blocks`: a newline after a `%}` is kept unless you write `-%}`.
* One trailing newline (`\n` or `\r\n`) at the very end of the template is dropped when the template ends with text (as Jinja does).

```text
a   {{- "x" -}}   b                 => axb
a\n{%- if t -%}\n  x\n{%- endif -%}\nb   => axb
line\n                              => line
line\n\n                            => line\n
```

### `raw`

`{% endraw %}{{ lt }} raw %}{% raw %}` ... `{% endraw %}{{ lt }} endraw %}{% raw %}` emits everything between them as text (`{{ not rendered }}`, `{% if %}`); `-` whitespace control works on both tags.

## Printing values

`{{ value }}` prints with the default formatter unless one was set with `Environment::set_formatter`:

| Value | Printed as |
|---|---|
| string | as is |
| integer | decimal (`3`, `-7`) |
| float | with a fractional part when it is whole (`2.0`, `1000.0`), otherwise Rust's shortest form (`0.30000000000000004`) |
| `true` / `false` | `true` / `false` (lower case) |
| null / `none` | `none` |
| undefined | nothing |
| list | `[1, "a"]` (strings quoted with JSON escapes, `, ` separator) |
| map | `{"k": "v"}` (in insertion order) |

## Literals

| Literal | Notes |
|---|---|
| `42`, `-3` | integers (`i64` range in the source; larger literals are `bad number`) |
| `1.5`, `1e3`, `2.0` | floats; `1e3` is `1000.0` |
| `"a"`, `'a'` | strings; escapes `\n \t \r`, any other `\c` is `c`; adjacent literals concatenate (`'a' "b"` is `ab`) |
| `true`, `True`, `false`, `False` | booleans |
| `none`, `None` | null |
| `[1, 2]` | list (trailing comma allowed) |
| `(1, 2)` | tuple, which is a list |
| `{"a": 1}` | map; keys are printed to strings (`display`) |

## Variables and lookup

A name is looked up in the template's scopes (innermost first: `for`, `with` and macro scopes, then the top-level `set`s and macros), then in the render context (when it is a map), then in the environment globals. A render context entry therefore wins over a global, and a `set` wins over both.

A name that is not found is **undefined**: it prints as nothing, is false, and iterates as an empty list. Reading an attribute of an undefined value (`missing.x`) is an error of kind `Undefined`; `default` and `is defined` accept it (see [internals](internals.md#undefined-values)).

### Attributes and subscripts

| Expression | Result |
|---|---|
| `map.key`, `map["key"]` | the entry, or undefined if absent |
| `list[0]`, `list.0`, `list[-1]` | element; negative counts from the end; out of range is undefined |
| `"text"[0]` | the character as a string (negative index allowed) |
| `x.y` where `x` is not a map | undefined (for `none`, a number, a list...) |
| `module.name` | an item of an imported module |

### Slices

`x[a:b]`, `x[a:b:c]`, `x[::-1]` on lists and strings, with Python semantics (negative bounds count from the end, bounds are clamped, step 0 is an error). A string slice gives a string. A bound must be an integer or none. More than three parts is `too many `:` in a slice`.

## Operators

From lowest to highest precedence:

| Level | Operators |
|---|---|
| conditional | `a if cond else b`; `a if cond` (undefined when false) |
| `or` | `or` (returns the first truthy operand, like Python) |
| `and` | `and` (returns the first falsy operand) |
| `not` | `not x` |
| comparison | `==  !=  <  <=  >  >=  in  not in` (chains: `a < b < c` is parsed left to right as `(a < b) < c`) |
| sum | `+  -` |
| concatenation | `~` (stringifies both sides) |
| product | `*  /  //  %` |
| power | `**` (right associative: `2 ** 3 ** 2` is `512`). Its left operand is a unary expression, so `-2 ** 2` is `(-2) ** 2` = `4`, **not** `-4` as in Python and Jinja |
| unary | `-x`  `+x` |
| postfix | `.attr`  `[i]`  `(args)`  `|filter`  `is test` |

`~` binds tighter than `+`/`-` and looser than `*`/`/`: `"a" ~ 1 + 2` is `("a" ~ 1) + 2`, which fails with `cannot apply `+` to string and number`. Comparisons chain left to right as in the table: `3 > 2 > 1` is `(3 > 2) > 1`, which is `false` (a boolean counts as 0/1), unlike Python. Use parentheses when mixing.

Semantics (verified by rendering):

| Expression | Result |
|---|---|
| `7 + 2`, `7 - 2`, `7 * 2` | `9`, `5`, `14` (integers stay integers; overflow past `i64`/`u64` is `integer overflow`) |
| `7 / 2` | `3.5` (always a float); `10 / 5` is `2.0` |
| `7 // 2`, `-7 // 2` | `3`, `-4` (floor) |
| `7 % 3`, `-7 % 3` | `1`, `2` (the result takes the sign of the divisor) |
| `2 ** 10`, `2 ** -1` | `1024`, `0.5` |
| `1 / 0`, `1 // 0`, `x % 0` | `invalid operation: division by zero` |
| `"a" + "b"`, `[1] + [2]` | `ab`, `[1, 2]` |
| `"ab" * 2`, `[1] * 3` | `abab`, `[1, 1, 1]` (an integer on either side) |
| `"a" ~ 1 ~ true` | `a1true` |
| `1 == 1.0` | `true` (numbers compare by value); lists and maps compare element-wise |
| `1 < 2`, `"a" < "b"` | `true`, `true` (numbers, strings, lists; `none` with `none`) |
| `"a" < 1` | `invalid operation: cannot compare string and number` |
| `"a" - 1` | `invalid operation: cannot apply `-` to string and number` |
| `2 in [1, 2]`, `"x" in "xyz"`, `"k" in {"k": 1}` | `true` (substring for strings, element for lists, key for maps) |
| `x in 5` | error: `cannot look for a value in a number` |
| `not t`, `t and n`, `nothing or "alt"` | `false`, `3`, `alt` |
| `"yes" if t else "no"` | `yes`; with no `else` and a false condition the result is undefined (prints nothing) |

Truthiness: `none`, `false`, `0`, `0.0`, `""`, `[]` and `{}` are false; everything else is true; undefined is false.

## Filters and tests in expressions

`value|name`, `value|name(arg, key=value)`: a filter, see [built-ins](builtins.md). Filters chain (`x|trim|upper`). `x is name`, `x is not name`, `x is name(arg)` and `x is name arg` (a single literal number or string argument without parentheses) are tests.

`default` (alias `d`) is built into the evaluator, not the filter table: `x|default(fallback)`, `x|default(fallback, true)` and the keywords `default_value=`/`boolean=`.

## Calls

* Functions: `range(...)`, `dict(...)` and anything registered with `Environment::add_function`; resolved only when the name is **not** a variable.
* Methods on values: `"a b".split()`, `map.items()`, ... see [built-ins](builtins.md#methods).
* Macros: `{{ m(1, b=2) }}`, `{{ module.m() }}`.
* Keyword arguments must come after positional ones (`positional argument after a keyword argument`).

## Statements

### `if` / `elif` / `else`

```text
{% if n > 5 %}big{% elif n > 2 %}mid{% else %}small{% endif %}      => mid     (n = 3)
```

### `for`

```text
{% for u in users %}{{ loop.index }}/{{ loop.length }}:{{ u.name }}{% if not loop.last %}, {% endif %}{% endfor %}
    => 1/2:ada, 2/2:bob
{% for x in [] %}x{% else %}none{% endfor %}                        => none
{% for x in list if x > 1 %}{{ x }} {% endfor %}                  => 3 2     (list = [3, 1, 2])
{% for k, v in map|dictsort %}{{ k }}={{ v }};{% endfor %}          => a=1;b=2;
{% for (k, v) in map.items() %}{{ k }}={{ v }};{% endfor %}         => b=2;a=1;
{% for c in "ab" %}{{ c }}{% endfor %}                              => ab
{% for k in map %}{{ k }}{% endfor %}                               => ba      (keys, in insertion order)
```

* Iterates a list, the keys of a map, the characters of a string; `none` and undefined iterate as empty; any other value is `... is not iterable`.
* `else` runs when there were no items (after the `if` filter).
* The optional `if` condition filters items before the loop starts, so `loop.length` counts only the kept ones. The condition is parsed with `or`-level precedence (no inline `a if b else c` inside it).
* Targets: one name, `a, b` or `(a, b)`; each item must then be a list of exactly that many elements (`wrong number of values to unpack`).
* Variables set inside the loop body are local to the loop (`{% set x = 2 %}` inside does not change an outer `x`).

The `loop` variable (a map, re-created per iteration): `index` (1-based), `index0`, `first`, `last`, `length`, `revindex`, `revindex0`, `previtem` (none for the first), `nextitem` (none for the last). There is no `loop.cycle`, `loop.changed` or `loop(...)` recursion.

### `set`

```text
{% set a = 1 %}{% set b, c = [2, 3] %}{{ a }}{{ b }}{{ c }}        => 123
{% set greeting %}Hi {{ name }}{% endset %}[{{ greeting }}]          => [Hi ada lovelace]
```

`set` with several targets unpacks a list of that length. The block form captures the rendered body as a string. A `set` at the top of a template writes the template's top scope (visible to macros defined there and exported to importers); inside `for`/`with`/macro it writes that inner scope.

### `with`

```text
{% with x = 5 %}{{ x }}{% endwith %}[{{ x }}]                       => 5[]
```

`name = expr, name2 = expr` bound in a new scope. The expressions are evaluated **before** the bindings exist, so one binding cannot use another from the same `with`.

### `macro`

```text
{% macro hi(who, punct="!") %}Hello {{ who }}{{ punct }}{% endmacro %}{{ hi("bob") }} {{ hi("amy", punct="?") }} {{ hi(who="kim") }}
    => Hello bob! Hello amy? Hello kim!
```

* Parameters with optional defaults (`name=expr`). A missing parameter without default is undefined (prints nothing).
* Too many positional arguments: `` macro `m` takes 1 arguments ``; an unknown keyword: `` macro `m` has no argument `b` ``.
* A macro returns its rendered text as a string. It sees the top-level scope of the template that defined it (its `set`s and other macros), its parameters, the render context of the caller and the globals, but **not** the caller's loop or `with` variables. Macros may call other macros of the same template and themselves, up to depth 32 (`macros call each other too deeply`).
* No `caller()`, `varargs`, `kwargs`.

### `import`, `from ... import`

```text
{% import "macros.j2" as m %}{{ m.greet("Ada") }} {{ m.shared }}
{% from "macros.j2" import greet as g, twice %}{{ g("D") }}
```

The named template is rendered once (output discarded) with **no render context** (only globals), and its top-level `set`s and macros become the module's attributes. `from ... import a, b as c` copies names (missing name: `the template has no `x` to import`). Import depth is limited to 32. The template expression may be any expression (a variable is allowed).

### `include`

```text
{% include "part.j2" %}
{% include "part.j2" ignore missing %}
```

Renders the named template inline with the **current** variables (a copy of the scope chain with a fresh innermost scope, so sets inside do not leak out). `ignore missing` swallows `TemplateNotFound` only. `with context`/`without context` are accepted and ignored: the context is always passed. Include depth is limited to 32.

## Not supported {#not-supported}

| Construct | Result |
|---|---|
| `extends`, `block`, `endblock`, `call`, `filter` blocks, `do`, `break`, `continue`, `autoescape`, `trans` | `` syntax error: unknown tag `name` `` |
| a stray `endif`/`endfor`/... | `unknown tag` |
| `loop.cycle`, `loop.changed`, recursive `loop(...)` | undefined / not callable |
| `namespace()`, `cycler()`, `joiner()` | `no function named ...` |
| autoescape, `|safe` semantics | not implemented: `escape`, `e`, `safe`, `forceescape` return the input unchanged |
| missing `endif`/`endfor`/`endmacro`... | `` syntax error: missing `endif` `` for an `if`; `` missing `endfor` `` for a `for`; `` missing `endmacro` ``, `` missing `endset` ``, `` missing `endwith` `` |
{% endraw %}
