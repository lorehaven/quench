---
title: Errors
parent: quench-template
grand_parent: Crates
nav_order: 3
---
{% raw %}
`Error` has a `kind()`, a `message()`, an optional `template()` name and a `line()` (1-based, 0 = unknown). `Display` is

```text
<what>: <message> (in <template>:<line>)      with template and line
<what>: <message> (in <template>)             template known, line 0
<what>: <message> (line <n>)                  no template, line known
<what>: <message>                             neither
```

where `<what>` is by kind: `syntax error`, `undefined value`, `invalid operation`, `template not found`, `unknown name`, `error` (Custom).

| Kind | Raised when | Message examples (from real runs) |
|---|---|---|
| `Syntax` | the template text is invalid (at `add_template`, `render_str` or first load) | `` unclosed `{{` ``; `` unknown tag `extends` ``; `unexpected None in an expression` (end of expression where a value was needed); `` expected `)` ``; `positional argument after a keyword argument`; `` too many `:` in a slice ``; `` bad number `18446744073709551615` ``; `unterminated string`; `` unexpected `@` ``; `` unclosed `raw` ``; `` expected `in` ``; `` expected `as` ``; `` expected `import` ``; `` expected `missing` ``; `` missing `elif` `` (see quirks) |
| `Undefined` | an attribute, method or index of an undefined value, or a non-`default` use of one where data is required | `cannot read `x` of an undefined value`; `cannot index an undefined value`; `cannot call `m` on an undefined value`; `the value is undefined`; `` `odd` was applied to an undefined value `` |
| `InvalidOperation` | a type error or a limit | `division by zero`; `cannot apply `-` to string and number`; `cannot compare string and number`; `integer overflow`; `expected a string, found number`; `number is not iterable`; `number has no length`; `string has no method `x``; `macro `m` takes 1 arguments`; `macro `m` has no argument `b``; `macros call each other too deeply`; `templates include each other too deeply`; `templates import each other too deeply`; `wrong number of values to unpack`; `cannot unpack a value that is not a sequence`; `the template has no `x` to import`; `range is too long`; `slice step cannot be zero`; `a macro or module cannot be printed` / `used as a value`; `cannot look for a value in a number` |
| `TemplateNotFound` | `template`/`render`/`import`/`include` of an unknown name with no loader result | `` `name` `` |
| `UnknownName` | no such filter, test or function | `` no filter named `x` ``; `` no test named `x` ``; `` no function named `x` `` |
| `Custom` | whatever your filter/test/function/loader returns | your message |

Where the location comes from: parse errors carry the line of the tag and the template name; render errors get the name and the line of the `{{ }}`/`set`/`for`/`import`/`include` statement being run (the innermost one wins; once set, outer frames do not overwrite).

## Known quirks {#known-quirks}

* A missing closer reports the **first** keyword the parser was waiting for: an unterminated `{% if %}` says ``missing `elif` `` (line 0), and an unterminated `for` says ``missing `else` ``. (`syntax.rs` `body`: `ends[0]`.)
* A pure render error inside a macro keeps the template name of the macro's definition.
* An error raised by a custom filter has its template and line filled in like any other.
{% endraw %}
