---
title: Built-ins
parent: quench-template
grand_parent: Crates
nav_order: 4
---
{% raw %}
Every built-in filter, test, function and method, from `src/builtins.rs` and `src/render.rs`. Outputs are real: they were produced by rendering the shown template with this context (a `json!` value), unless noted.

```json
{ "name": "ada lovelace", "n": 3, "neg": -7, "f": 2.5, "t": true, "nothing": null,
  "list": [3, 1, 2], "words": ["banana", "Apple", "cherry"], "map": {"b": 2, "a": 1},
  "users": [{"name": "ada", "age": 36, "admin": true}, {"name": "bob", "age": 25, "admin": false}],
  "text": "  Hello, World  ", "multi": "a\nb\nc", "url": "a b/c?d" }
```

Maps keep their insertion order, so `map|keys` is `["b", "a"]`.

Arguments are positional or by keyword; in each signature below the keyword is the parameter name. A user filter registered with `add_filter` with the same name **replaces** the built-in one; so do `add_function` and `add_test`.

## Filters

A filter that needs a string and gets another type fails with `invalid operation: expected a string, found number` (`found` is one of `none`, `bool`, `number`, `string`, `sequence`, `map`).

### String filters

| Filter | Signature | Example => result |
|---|---|---|
| `upper` | `upper` | `{{ name\|upper }}` => `ADA LOVELACE` |
| `lower` | `lower` | `{{ "Hello"\|lower }}` => `hello` |
| `title` | `title` | `{{ name\|title }}` => `Ada Lovelace` (each alphanumeric run gets a capital first letter, the rest lower case) |
| `capitalize` | `capitalize` | `{{ "hELLO wORLD"\|capitalize }}` => `Hello world` |
| `trim`, `strip` | `trim(chars)` | `{{ text\|trim }}` => `Hello, World`; `{{ "xxhixx"\|trim("x") }}` => `hi` (`chars` is a set of characters) |
| `replace` | `replace(old, new, count)` | `{{ "a-b-c"\|replace("-", "+") }}` => `a+b+c`; with count 1 => `a+b-c` |
| `indent` | `indent(width=4, first=false, blank=false)` | `{{ multi\|indent(2) }}` => `a`, `  b`, `  c` on three lines; `indent(2, true)` indents the first line too; `indent("> ", first=true)` uses a string; empty lines are not indented unless `blank` |
| `truncate` | `truncate(length=255, killwords=false, end="...")` | `{{ "hello world foo"\|truncate(10) }}` => `hello...`; `truncate(10, true)` => `hello w...`; `truncate(10, false, "~")` => `hello~`. Text no longer than `length` characters is returned unchanged. The cut is at `length - len(end)` characters, then back to the last space unless `killwords`. |
| `urlencode` | `urlencode` | `{{ url\|urlencode }}` => `a%20b/c%3Fd` (letters, digits and `- _ . ~ /` are kept; other bytes become `%XX`) |
| `split` | `split(sep, maxsplit)` | `{{ "a,b,c"\|split(",") }}` => `["a", "b", "c"]`; `split(",", 1)` => `["a", "b,c"]`; with no separator it splits on whitespace runs |
| `escape`, `e`, `safe`, `forceescape` | none | return the value **unchanged** (nothing is escaped) |
| `string` | `string` | `{{ 42\|string }}` => `42`; `{{ [1,2]\|string }}` => `[1, 2]` (the printed form) |
| `tojson` | `tojson` | `{{ "a"\|tojson }}` => `"a"`; `{{ users\|tojson }}` => compact JSON `[{"name":"ada","age":36,"admin":true},...]` (no spaces) |

### Sequence and map filters

`items()` below means: a list is itself, a map is its keys, a string its characters, `none` and undefined are empty; any other type is `... is not iterable`.

| Filter | Signature | Example => result |
|---|---|---|
| `length`, `count` | `length` | string: characters; list: elements; map: entries; else `... has no length`. `{{ name\|length }}` => `12` |
| `list` | `list` | `{{ "abc"\|list }}` => `["a", "b", "c"]`; `{{ map\|list }}` => `["b", "a"]` |
| `first`, `last` | `first` | `{{ list\|first }}` => `3`; `{{ list\|last }}` => `2`; empty gives none |
| `reverse` | `reverse` | `{{ "abc"\|reverse }}` => `cba`; `{{ list\|reverse }}` => `[2, 1, 3]` |
| `keys`, `values`, `items` | none | maps only: `{{ map\|keys }}` => `["b", "a"]`; `values` => `[2, 1]`; `items` => `[["b", 2], ["a", 1]]` |
| `dictsort` | `dictsort(case_sensitive=false, by="key", reverse=false)` | maps only: `{{ map\|dictsort }}` => `[["a", 1], ["b", 2]]`; `dictsort(false, "value", true)` => `[["b", 2], ["a", 1]]`. Without `case_sensitive`, string keys/values compare in lower case. |
| `sort` | `sort(reverse=false, case_sensitive=false, attribute)` | `{{ list\|sort }}` => `[1, 2, 3]`; `sort(true)` => `[3, 2, 1]`; `{{ words\|sort }}` => `["Apple", "banana", "cherry"]`; `{{ users\|sort(false, false, "age")\|map(attribute="name") }}` => `["bob", "ada"]`. Stable; values that cannot be compared count as equal. |
| `unique` | `unique` | `{{ [1, 2, 1, 3, 2]\|unique }}` => `[1, 2, 3]` (first occurrences, numeric equality) |
| `join` | `join(d="", attribute)` | `{{ list\|join(", ") }}` => `3, 1, 2`; `{{ users\|join(", ", "name") }}` => `ada, bob`. Items print in their display form. |
| `sum` | `sum(attribute, start=0)` | `{{ list\|sum }}` => `6`; `sum(start=10)` => `16`; `{{ users\|sum("age") }}` => `61`; uses `+`, so strings and lists concatenate |
| `min`, `max` | none | `{{ list\|min }}` => `1`; `max` => `3`; empty gives none |
| `batch` | `batch(linecount=1, fill_with)` | `{{ list\|batch(2) }}` => `[[3, 1], [2]]`; `batch(2, 0)` => `[[3, 1], [2, 0]]` |
| `attr` | `attr(name)` | `{{ users[0]\|attr("name") }}` => `ada`; the name may be a dotted path (`"a.b.0"`); missing gives none |
| `map` | `map(filter, args...)` or `map(attribute=, default=)` | `{{ words\|map("upper") }}` => `["BANANA", "APPLE", "CHERRY"]`; `{{ users\|map(attribute="name") }}` => `["ada", "bob"]`; `map(attribute="nick", default="-")` => `["-", "-"]` |
| `select`, `reject` | `select(test, args...)` | `{{ [1, 2, 3, 4]\|select("odd") }}` => `[1, 3]`; `reject("odd")` => `[2, 4]`; with no test, truthiness: `{{ [0, 1, "", "a"]\|select }}` => `[1, "a"]` |
| `selectattr`, `rejectattr` | `selectattr(attribute, test, args...)` | `{{ users\|selectattr("admin") }}` keeps the admin user; `{{ users\|selectattr("age", "gt", 30)\|map(attribute="name") }}` => `["ada"]`. The test is any test of the table below. |

`map` with an attribute only reads keyword `attribute=`; `selectattr` reads the attribute name from the first positional argument. The attribute of `sort`, `join`, `sum`, `attr` may be a dotted path through maps and list indexes.

### Number filters

| Filter | Signature | Example => result |
|---|---|---|
| `abs` | `abs` | `{{ neg\|abs }}` => `7` |
| `round` | `round(precision=0, method="common")` | **always a float**: `{{ 3.14159\|round(2) }}` => `3.14`; `{{ 2.5\|round }}` => `3.0` (half away from zero); `round(0, "floor")` => `2.0`; `{{ 2.1\|round(0, "ceil") }}` => `3.0`. Any other method name is the common method. |
| `int` | `int(default=0)` | `{{ "12"\|int }}` => `12`; `{{ "12.7"\|int }}` => `12`; `{{ 3.9\|int }}` => `3`; `{{ "x"\|int }}` => `0`; `{{ "x"\|int(5) }}` => `5` |
| `float` | `float(default=0.0)` | `{{ "1.5"\|float }}` => `1.5`; `{{ 3\|float }}` => `3.0`; `{{ "x"\|float }}` => `0.0` |

### `default` (alias `d`)

`default(fallback="", boolean=false)`. Implemented in the evaluator so that it accepts undefined values:

| Template | Result |
|---|---|
| `{{ missing\|default("fallback") }}` | `fallback` |
| `{{ ""\|default("fallback") }}` | *(empty: an empty string is defined)* |
| `{{ ""\|default("fallback", true) }}` | `fallback` (`boolean=true` also replaces false values) |
| `{{ missing.x\|default("deep") }}` | `deep` (an attribute of an undefined value is accepted here) |
| `{{ nothing\|d("x") }}` | `none` (a null is defined) |

The fallback may itself be undefined, then the result stays undefined. Keywords: `default_value=`, `boolean=`.

## Tests

`x is name`, `x is not name`, `x is name(arg)`, `x is name arg`. `defined` and `undefined` accept undefined values; **every other test on an undefined value is an error** (`... was applied to an undefined value`). Missing argument: `test `x` needs an argument`.

| Test | True when |
|---|---|
| `defined`, `undefined` | the value exists / does not exist (`none` is defined) |
| `none` | the value is null |
| `string`, `number`, `integer`, `float`, `boolean`, `mapping` | the type (`number` is int or float; `integer` int/uint) |
| `sequence` | a list **or a string** |
| `iterable` | list, string or map |
| `true`, `false` | the boolean itself |
| `callable` | never (always false) |
| `odd`, `even` | an integer that is odd/even (false for non-integers) |
| `divisibleby(n)` | integers, `n` not 0; otherwise error `divisibleby needs non-zero integers` |
| `eq`, `equalto`, `sameas` (`x`) | equal by value |
| `ne` | not equal |
| `lt`/`lessthan`, `le`, `gt`/`greaterthan`, `ge` | ordered comparison; false if the types are not comparable |
| `in(container)` | `x` is an element/substring/key of the container |
| `lower`, `upper` | a string with no upper-case / no lower-case characters |
| `startingwith(p)`, `endingwith(s)` | string prefix/suffix |

```text
{{ 3 is odd }} {{ 9 is divisibleby 3 }} {{ 2 is in([1, 2]) }} {{ "abc" is startingwith "ab" }} {{ n is not none }}
   => true true true true true
```

An unknown test: `` unknown name: no test named `x` ``.

## Functions

| Function | Behaviour |
|---|---|
| `range(stop)`, `range(start, stop)`, `range(start, stop, step)` | list of integers; `{{ range(3) }}` => `[0, 1, 2]`; `range(10, 0, -3)` => `[10, 7, 4, 1]`; step 0 is an error; more than 100 000 items is `range is too long`; non-numbers fail |
| `dict(a=1, b="x")` | a map from the keyword arguments: `{"a": 1, "b": "x"}` |

A function is looked up only when the called name is **not** a variable. User functions from `add_function` win over these. An unknown one: `` no function named `x` ``.

## Methods {#methods}

Called with `value.method(args)`. A method that does not exist for the type: `` invalid operation: string has no method `x` ``.

| On | Methods |
|---|---|
| string | `upper()`, `lower()`, `strip()`, `lstrip()`, `rstrip()`, `startswith(prefix)`, `endswith(suffix)`, `split(sep)` (no argument: whitespace runs), `replace(old, new)`, `join(sequence)` (`", ".join(["a","b"])` => `a, b`), `capitalize()`, `title()` |
| map | `items()` => list of `[key, value]`; `keys()`; `values()`; `get(key, default)` (default `none` when omitted) |

Examples: `{{ "  x ".strip() }}` => `x`; `{{ "a,b".split(",") }}` => `["a", "b"]`; `{{ map.get("z", "dflt") }}` => `dflt`; `{{ map.get("z") }}` => `none`.
{% endraw %}
