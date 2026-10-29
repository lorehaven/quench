# Quench Template

`quench-template` (crate `quench_template`) is a Jinja-compatible template engine over `quench_serde::Value`. It replaces `minijinja` and removes the serde bridge: the render context, globals and filter arguments are plain `Value`s.

```rust
use quench_serde::json;
use quench_template::Environment;

let mut env = Environment::new();
env.add_global("env", "prod");
{% raw %}env.add_template("page", "{% for t in tags %}{{ t|upper }}{% if not loop.last %}, {% endif %}{% endfor %} in {{ env }}")?;{% endraw %}
let out = env.render("page", &json!({ "tags": ["a", "b"] }))?;   // "A, B in prod"
```

## Language

- {% raw %}Tags `{{ }}`, `{% %}`, `{# #}`; `-` trims whitespace on that side (`{{- x -}}`); `{% raw %}`; one trailing newline of a template is dropped, as in Jinja. Output is never escaped.{% endraw %}
- Statements: `if`/`elif`/`else`, `for` (tuple targets, `else`, an `if` filter, the `loop` variable with `index`, `index0`, `revindex`, `revindex0`, `first`, `last`, `length`, `previtem`, `nextitem`), `set` (also tuple and block form), `with`, `macro` (defaults, keyword arguments), `import ... as`, `from ... import ... as`, `include` (`ignore missing`). `extends`/`block`/`call` are not supported.
- Expressions: literals (including lists, dicts, tuples), `+ - * / // % **`, `~`, comparisons, `in`/`not in`, `and`/`or`/`not`, `a if c else b`, subscripts and slices (`x[1:3]`, `x[::-1]`), attribute access, calls, filters (`x|f(args)`), tests (`x is defined`, `x is not none`, `x is divisibleby 3`).
- Undefined values: a missing variable prints as nothing, is false and iterates as empty; reading an attribute of it is an `Undefined` error, which `|default(...)` and `is defined` accept.
- Filters: `abs attr batch capitalize count default dictsort e escape first float int items join keys last length list lower map max min reject rejectattr replace reverse round safe select selectattr sort split string strip sum title tojson trim truncate unique upper urlencode values indent`. Tests: `defined undefined none string number integer float boolean mapping sequence iterable true false odd even divisibleby eq ne lt le gt ge in lower upper startingwith endingwith callable`. Functions: `range`, `dict`. Methods: strings (`split strip lstrip rstrip upper lower startswith endswith replace join capitalize title`) and maps (`items keys values get`).

## API

- `Environment::{new, add_template, set_loader, add_global, add_filter, add_test, add_function, set_formatter, template, render, render_str}`.
- `set_loader(|name| Ok(Some(source)))` finds templates not added up front, for `import`, `include` and `render`.
- `add_filter(name, |value, args| ...)`: `Args::get(index, keyword)` reads positional or keyword arguments.
- `set_formatter(|value| Some(text))` overrides how `{% raw %}{{ value }}{% endraw %}` prints (return `None` for the default), for example to print booleans as `true`/`false`.
- `Error` has a `kind()` (`Syntax`, `Undefined`, `InvalidOperation`, `TemplateNotFound`, `UnknownName`, `Custom`), the template name and line.

Maps keep the order of the `Value` they come from (minijinja sorted keys); sort explicitly with `|dictsort` where order matters. Macro and include nesting is limited to 32 levels.
