//! The template engine: syntax, expressions, filters, tests, loops, macros and errors.

#![allow(clippy::too_many_lines)]

use quench_serde::{Value, json};
use quench_template::{Args, Environment, ErrorKind};

fn render(source: &str, context: &Value) -> String {
    Environment::new()
        .render_str(source, context)
        .unwrap_or_else(|e| panic!("{source}: {e}"))
}

fn r(source: &str) -> String {
    render(source, &json!({}))
}

fn fails(source: &str, context: &Value) -> quench_template::Error {
    Environment::new().render_str(source, context).unwrap_err()
}

#[test]
fn text_variables_and_comments() {
    let ctx = json!({"name": "ada", "n": 3, "f": 1.5, "t": true, "none": null, "list": [1, "a"], "map": {"k": "v"}});
    assert_eq!(render("hi {{ name }}{# gone #}!", &ctx), "hi ada!");
    assert_eq!(
        render("{{ n }} {{ f }} {{ t }} {{ none }}", &ctx),
        "3 1.5 true none"
    );
    assert_eq!(
        render("{{ list }} {{ map }}", &ctx),
        "[1, \"a\"] {\"k\": \"v\"}"
    );
    assert_eq!(r("{{ 2.0 }} {{ 1e3 }}"), "2.0 1000.0");
    assert_eq!(r("{{ missing }}|"), "|");
    // One trailing newline is dropped, like Jinja.
    assert_eq!(r("a\n"), "a");
    assert_eq!(r("a\r\n"), "a");
    assert_eq!(r("a\n\n"), "a\n");
}

#[test]
fn whitespace_control_and_raw() {
    assert_eq!(r("a  {{- 1 -}}  b"), "a1b");
    assert_eq!(r("a\n{%- if true -%}\n  x\n{%- endif %}\nb"), "ax\nb");
    assert_eq!(r("a {#- c -#} b"), "ab");
    assert_eq!(
        r("{% raw %}{{ not }} {% x %}{% endraw %}!"),
        "{{ not }} {% x %}!"
    );
    assert_eq!(r("a {%- raw -%} {{ x }} {%- endraw -%} b"), "a{{ x }}b");
    assert_eq!(r("{{ '}}' }}"), "}}");
}

#[test]
fn expressions() {
    assert_eq!(
        r("{{ 1 + 2 * 3 }} {{ (1 + 2) * 3 }} {{ 7 // 2 }} {{ 7 % 3 }} {{ 2 ** 10 }} {{ 7 / 2 }}"),
        "7 9 3 1 1024 3.5"
    );
    assert_eq!(
        r("{{ -7 // 2 }} {{ -7 % 3 }} {{ 7.5 // 2 }} {{ 7.5 % 2 }} {{ 2 ** -1 }} {{ 1 + 1.5 }}"),
        "-4 2 3.0 1.5 0.5 2.5"
    );
    assert_eq!(
        r("{{ 'a' ~ 1 ~ 'b' }} {{ 'ab' * 2 }} {{ [1] + [2] }} {{ [0] * 2 }}"),
        "a1b abab [1, 2] [0, 0]"
    );
    assert_eq!(
        r(
            "{{ 1 < 2 }} {{ 2 <= 2 }} {{ 'a' < 'b' }} {{ 1 == 1.0 }} {{ 1 != 2 }} {{ [1] < [2] }} {{ 3 >= 4 }} {{ 3 > 2 }}"
        ),
        "true true true true true true false true"
    );
    assert_eq!(
        r("{{ 1 in [1, 2] }} {{ 'a' in 'abc' }} {{ 'k' in {'k': 1} }} {{ 3 not in [1, 2] }}"),
        "true true true true"
    );
    assert_eq!(
        r(
            "{{ 1 and 0 }} {{ 0 or 'x' }} {{ not 0 }} {{ 'a' if 1 else 'b' }} {{ 'a' if 0 else 'b' }}|{{ 'a' if 0 }}|"
        ),
        "0 x true a b||"
    );
    assert_eq!(
        r(
            "{{ [1, 2, 3][1] }} {{ [1, 2, 3][-1] }} {{ 'abc'[1] }} {{ {'a': 1}['a'] }} {{ {'a': 1}.a }} {{ [[1, 2]][0].1 }}"
        ),
        "2 3 b 1 1 2"
    );
    assert_eq!(
        r(
            "{{ [1, 2, 3, 4][1:3] }} {{ [1, 2, 3][::-1] }} {{ 'abcdef'[::2] }} {{ [1, 2, 3][:-1] }} {{ [1, 2, 3][1:] }} {{ [1,2,3][-2:] }}"
        ),
        "[2, 3] [3, 2, 1] ace [1, 2] [2, 3] [2, 3]"
    );
    assert_eq!(
        r("{{ (1, 2) }} {{ 'a' 'b' }} {{ 'it\\'s' }} {{ \"q\\\"\" }}"),
        "[1, 2] ab it's q\""
    );
    assert_eq!(
        r("{{ range(3) }} {{ range(1, 4) }} {{ range(0, 6, 2) }} {{ range(3, 0, -1) }}"),
        "[0, 1, 2] [1, 2, 3] [0, 2, 4] [3, 2, 1]"
    );
    assert_eq!(
        r("{{ dict(a=1, b=2) }} {{ none is none }} {{ True }} {{ False }} {{ None }}"),
        "{\"a\": 1, \"b\": 2} true true false none"
    );
}

#[test]
fn control_flow() {
    let ctx = json!({"items": ["a", "b", "c"], "map": {"x": 1, "y": 2}, "pairs": [[1, 2], [3, 4]], "empty": []});
    assert_eq!(
        render("{% for i in items %}{{ i }}{% endfor %}", &ctx),
        "abc"
    );
    assert_eq!(
        render(
            "{% for i in items %}{{ loop.index }}{{ loop.index0 }}{{ loop.revindex }}{{ loop.revindex0 }}{{ loop.first }}{{ loop.last }}{{ loop.length }},{% endfor %}",
            &ctx
        ),
        "1032truefalse3,2121falsefalse3,3210falsetrue3,"
    );
    assert_eq!(render("{% for k in map %}{{ k }}{% endfor %}", &ctx), "xy");
    assert_eq!(
        render("{% for a, b in pairs %}{{ a + b }},{% endfor %}", &ctx),
        "3,7,"
    );
    assert_eq!(
        render(
            "{% for k, v in map|dictsort %}{{ k }}={{ v }};{% endfor %}",
            &ctx
        ),
        "x=1;y=2;"
    );
    assert_eq!(
        render(
            "{% for k, v in map.items() %}{{ k }}{{ v }}{% endfor %}",
            &ctx
        ),
        "x1y2"
    );
    assert_eq!(
        render("{% for i in empty %}x{% else %}none{% endfor %}", &ctx),
        "none"
    );
    assert_eq!(
        render("{% for i in nothing %}x{% else %}none{% endfor %}", &ctx),
        "none"
    );
    assert_eq!(
        render(
            "{% for i in range(6) if i is even %}{{ i }}{{ loop.length }},{% endfor %}",
            &ctx
        ),
        "03,23,43,"
    );
    assert_eq!(render("{% for c in 'ab' %}{{ c }}{% endfor %}", &ctx), "ab");
    assert_eq!(
        render(
            "{% for i in items %}{{ loop.previtem }}{{ loop.nextitem }};{% endfor %}",
            &ctx
        ),
        "noneb;ac;bnone;"
    );
    assert_eq!(r("{% if 0 %}a{% elif 1 %}b{% else %}c{% endif %}"), "b");
    assert_eq!(r("{% if 0 %}a{% elif 0 %}b{% else %}c{% endif %}"), "c");
    assert_eq!(r("{% if 0 %}a{% endif %}z"), "z");
    assert_eq!(r("{% if missing %}a{% else %}b{% endif %}"), "b");
}

#[test]
fn set_with_and_blocks() {
    assert_eq!(r("{% set a = 1 %}{% set b = a + 1 %}{{ a }}{{ b }}"), "12");
    assert_eq!(r("{% set a, b = [1, 2] %}{{ b }}{{ a }}"), "21");
    assert_eq!(r("{% set x %}in{{ 1 }}{% endset %}[{{ x }}]"), "[in1]");
    assert_eq!(
        r("{% with a = 1, b = 2 %}{{ a + b }}{% endwith %}{{ a }}|"),
        "3|"
    );
    assert_eq!(
        r("{% for i in [1] %}{% set t = i %}{% endfor %}{{ t }}|"),
        "|"
    );
    assert_eq!(r("{% set a = missing %}{{ a }}|"), "|");
}

#[test]
fn filters() {
    let ctx = json!({"s": "Hello World", "l": [3, 1, 2], "m": {"b": 2, "a": 1}, "words": ["x", "y"],
        "objs": [{"n": "a", "v": 2}, {"n": "b", "v": 1}], "pad": "  p  ", "ws": "a b  c"});
    let cases = [
        ("s|upper", "HELLO WORLD"),
        ("s|lower", "hello world"),
        ("s|title", "Hello World"),
        ("'hello world'|capitalize", "Hello world"),
        ("pad|trim", "p"),
        ("'xxpxx'|trim('x')", "p"),
        ("s|length", "11"),
        ("l|length", "3"),
        ("m|length", "2"),
        ("l|sort", "[1, 2, 3]"),
        ("l|sort(reverse=true)", "[3, 2, 1]"),
        ("l|first", "3"),
        ("l|last", "2"),
        ("l|reverse", "[2, 1, 3]"),
        ("'abc'|reverse", "cba"),
        ("l|sum", "6"),
        ("l|min", "1"),
        ("l|max", "3"),
        ("l|join('-')", "3-1-2"),
        ("words|join", "xy"),
        ("objs|join(', ', attribute='n')", "a, b"),
        ("objs|map(attribute='n')|list", "[\"a\", \"b\"]"),
        ("words|map('upper')|list", "[\"X\", \"Y\"]"),
        ("objs|sort(attribute='v')|map(attribute='n')|join", "ba"),
        ("objs|selectattr('v', 'gt', 1)|map(attribute='n')|join", "a"),
        ("objs|rejectattr('v', 'gt', 1)|map(attribute='n')|join", "b"),
        ("l|select('odd')|list", "[3, 1]"),
        ("l|reject('odd')|list", "[2]"),
        ("[0, 1, '']|select|list", "[1]"),
        ("m|dictsort", "[[\"a\", 1], [\"b\", 2]]"),
        (
            "m|dictsort(by='value', reverse=true)",
            "[[\"b\", 2], [\"a\", 1]]",
        ),
        ("m|keys|list", "[\"b\", \"a\"]"),
        ("m|values|list", "[2, 1]"),
        ("m|items", "[[\"b\", 2], [\"a\", 1]]"),
        ("s|replace('World', 'There')", "Hello There"),
        ("'aaa'|replace('a', 'b', 2)", "bba"),
        ("ws|split|join('|')", "a|b|c"),
        ("'a,b'|split(',')|join('|')", "a|b"),
        ("[1, 1, 2]|unique|list", "[1, 2]"),
        ("[1, 2, 3]|batch(2)", "[[1, 2], [3]]"),
        ("[1, 2, 3]|batch(2, 0)", "[[1, 2], [3, 0]]"),
        ("(-3)|abs", "3"),
        ("2.567|round(1)", "2.6"),
        ("2.5|round", "3.0"),
        ("2.567|round(1, 'floor')", "2.5"),
        ("2.51|round(1, 'ceil')", "2.6"),
        ("'12'|int", "12"),
        ("'x'|int", "0"),
        ("'x'|int(7)", "7"),
        ("2.9|int", "2"),
        ("'1.5'|float", "1.5"),
        ("3|float", "3.0"),
        ("1|string", "1"),
        ("'ab'|list", "[\"a\", \"b\"]"),
        ("m|tojson", "{\"b\":2,\"a\":1}"),
        ("'a b/c'|urlencode", "a%20b/c"),
        ("'<b>'|e", "<b>"),
        ("'<b>'|safe", "<b>"),
        ("'a\nb'|indent(2)", "a\n  b"),
        ("'a\nb'|indent(2, true)", "  a\n  b"),
        ("'a\n\nb'|indent(2)", "a\n\n  b"),
        ("'a\n\nb'|indent(2, false, true)", "a\n  \n  b"),
        ("'a\nb'|indent('--')", "a\n--b"),
        ("'a\nb'|indent", "a\n    b"),
        ("'hello world foo'|truncate(10)", "hello..."),
        ("'hello world foo'|truncate(10, true)", "hello w..."),
        ("'short'|truncate(10)", "short"),
        ("objs|first|attr('n')", "a"),
        ("missing|default('d')", "d"),
        ("missing.deep|default('d')", "d"),
        ("''|default('d')", ""),
        ("''|default('d', true)", "d"),
        ("missing|default", ""),
        ("missing|d(1)", "1"),
        ("missing|default(other)|default('z')", "z"),
    ];
    for (expr, expected) in cases {
        assert_eq!(
            render(&format!("{{{{ {expr} }}}}"), &ctx),
            expected,
            "{expr}"
        );
    }
}

#[test]
fn tests() {
    let ctx = json!({"s": "x", "n": 4, "f": 1.5, "l": [1], "m": {"a": 1}, "z": null, "t": true});
    let cases = [
        ("s is defined", true),
        ("missing is defined", false),
        ("missing is undefined", true),
        ("missing.deep is defined", false),
        ("z is none", true),
        ("s is string", true),
        ("n is number", true),
        ("n is integer", true),
        ("f is float", true),
        ("t is boolean", true),
        ("m is mapping", true),
        ("l is sequence", true),
        ("l is iterable", true),
        ("t is true", true),
        ("t is false", false),
        ("n is even", true),
        ("n is odd", false),
        ("n is divisibleby(2)", true),
        ("n is divisibleby 3", false),
        ("n is eq(4)", true),
        ("n is ne 4", false),
        ("n is lt(5)", true),
        ("n is le 4", true),
        ("n is gt(5)", false),
        ("n is ge 4", true),
        ("n is in([4, 5])", true),
        ("s is lower", true),
        ("s is upper", false),
        ("'abc' is startingwith 'a'", true),
        ("'abc' is endingwith('c')", true),
        ("s is callable", false),
        ("s is not string", false),
        ("missing is not defined", true),
    ];
    for (expr, expected) in cases {
        assert_eq!(
            render(&format!("{{{{ {expr} }}}}"), &ctx),
            expected.to_string(),
            "{expr}"
        );
    }
}

#[test]
fn methods() {
    assert_eq!(
        r("{{ 'a-b'.split('-') }} {{ ' x '.strip() }}{{ 'ab'.upper() }}{{ 'AB'.lower() }}"),
        "[\"a\", \"b\"] xABab"
    );
    assert_eq!(
        r(
            "{{ 'abc'.startswith('a') }} {{ 'abc'.endswith('x') }} {{ 'a b'.split() }} {{ '-'.join(['a', 'b']) }}"
        ),
        "true false [\"a\", \"b\"] a-b"
    );
    assert_eq!(
        r(
            "{{ 'aXa'.replace('a', 'b') }} {{ ' x'.lstrip() }}{{ 'x '.rstrip() }}|{{ 'hello'.capitalize() }} {{ 'a b'.title() }}"
        ),
        "bXb xx|Hello A B"
    );
    let ctx = json!({"m": {"a": 1}});
    assert_eq!(
        render(
            "{{ m.get('a') }}{{ m.get('b', 'd') }}{{ m.get('b') }}|{{ m.keys() }}{{ m.values() }}",
            &ctx
        ),
        "1dnone|[\"a\"][1]"
    );
}

#[test]
fn macros_imports_and_includes() {
    let mut env = Environment::new();
    env.add_template(
        "macros.j2",
        "{% macro greet(name, greeting='Hello') %}{{ greeting }}, {{ name }}!{% endmacro %}\
         {% macro twice(x) %}{{ greet(x) }}{{ greet(x, 'Bye') }}{% endmacro %}\
         {% set shared = 5 %}",
    )
    .unwrap();
    env.add_template("part.j2", "[{{ who }}]").unwrap();
    env.add_template(
        "main.j2",
        "{% import 'macros.j2' as m %}{{ m.greet('Ada') }} {{ m.greet(name='Bob', greeting='Hi') }} {{ m.twice('C') }} {{ m.shared }}\
         {% from 'macros.j2' import greet as g %} {{ g('D') }}\
         {% set who = 'me' %} {% include 'part.j2' %}{% include 'nope.j2' ignore missing %}",
    )
    .unwrap();
    assert_eq!(
        env.render("main.j2", &json!({})).unwrap(),
        "Hello, Ada! Hi, Bob! Hello, C!Bye, C! 5 Hello, D! [me]"
    );

    env.add_template(
        "local.j2",
        "{% macro a() %}A{% endmacro %}{% macro b() %}{{ a() }}B{% endmacro %}{{ b() }}",
    )
    .unwrap();
    assert_eq!(env.render("local.j2", &json!({})).unwrap(), "AB");
    env.add_template(
        "rec.j2",
        "{% macro f(n) %}{{ f(n) }}{% endmacro %}{{ f(1) }}",
    )
    .unwrap();
    assert!(
        env.render("rec.j2", &json!({}))
            .unwrap_err()
            .message()
            .contains("too deeply")
    );
    env.add_template("loop.j2", "{% include 'loop.j2' %}")
        .unwrap();
    assert!(env.render("loop.j2", &json!({})).is_err());
    env.add_template("imp.j2", "{% import 'imp.j2' as x %}")
        .unwrap();
    assert!(env.render("imp.j2", &json!({})).is_err());
}

#[test]
fn macro_argument_errors() {
    let mut env = Environment::new();
    env.add_template("m", "{% macro f(a, b=2) %}{{ a }}{{ b }}{% endmacro %}")
        .unwrap();
    for body in ["{{ f(1, 2, 3) }}", "{{ f(1, c=2) }}", "{{ f }}"] {
        env.add_template("t", &format!("{{% from 'm' import f %}}{body}"))
            .unwrap();
        assert!(env.render("t", &json!({})).is_err(), "{body}");
    }
    env.add_template("t", "{% from 'm' import f %}{{ f(1) }}|{{ f() }}")
        .unwrap();
    assert_eq!(env.render("t", &json!({})).unwrap(), "12|2");
    env.add_template("t", "{% from 'm' import nope %}").unwrap();
    assert!(env.render("t", &json!({})).is_err());
}

#[test]
fn environment_hooks() {
    let mut env = Environment::new();
    env.add_global("env", "prod");
    env.add_filter("shout", |v, args| {
        let suffix = args.get(0, "suffix").and_then(Value::as_str).unwrap_or("!");
        Ok(Value::String(format!(
            "{}{suffix}",
            v.as_str().unwrap_or_default().to_uppercase()
        )))
    });
    env.add_test("big", |v, _| Ok(v.as_i64().is_some_and(|n| n > 100)));
    env.add_function("twice", |args: &Args| {
        Ok(Value::from(args.positional[0].as_i64().unwrap_or(0) * 2))
    });
    env.set_formatter(|v| {
        matches!(v, Value::Bool(_))
            .then(|| format!("<{}>", if *v == Value::Bool(true) { "yes" } else { "no" }))
    });
    env.set_loader(|name| Ok((name == "dyn.j2").then(|| "dynamic {{ env }}".to_string())));
    env.add_template("t", "{{ env }} {{ 'a'|shout }} {{ 'a'|shout('?') }} {{ 500 is big }} {{ twice(4) }} {{ true }} {% include 'dyn.j2' %}").unwrap();
    assert_eq!(
        env.render("t", &json!({})).unwrap(),
        "prod A! A? <yes> 8 <yes> dynamic prod"
    );
    assert_eq!(
        env.render("dyn.j2", &json!({"env": "ctx"})).unwrap(),
        "dynamic ctx"
    );
    assert_eq!(env.template("dyn.j2").unwrap().name(), "dyn.j2");
    assert_eq!(
        env.render("nothing", &json!({})).unwrap_err().kind(),
        ErrorKind::TemplateNotFound
    );
    assert!(format!("{env:?}").contains("1 templates"));
    env.set_loader(|_| Ok(None));
    assert!(env.render("dyn.j2", &json!({})).is_err());
}

#[test]
fn errors_say_what_and_where() {
    let e = fails("a\n{{ 1 + }}", &json!({}));
    assert_eq!(e.kind(), ErrorKind::Syntax);
    assert!(e.to_string().contains("syntax error"), "{e}");
    assert_eq!(
        fails("{{ missing.attr }}", &json!({})).kind(),
        ErrorKind::Undefined
    );
    let e = fails("one\ntwo {{ 1 / 0 }}", &json!({}));
    assert_eq!(e.kind(), ErrorKind::InvalidOperation);
    assert_eq!((e.template(), e.line()), (Some("<string>"), 2));
    assert!(e.to_string().contains("<string>:2"), "{e}");
    assert_eq!(
        fails("{{ 'x'|nofilter }}", &json!({})).kind(),
        ErrorKind::UnknownName
    );
    assert_eq!(
        fails("{{ 1 is notatest }}", &json!({})).kind(),
        ErrorKind::UnknownName
    );
    assert_eq!(
        fails("{{ nofunc() }}", &json!({})).kind(),
        ErrorKind::UnknownName
    );
    assert_eq!(
        fails("{{ missing is eq 1 }}", &json!({})).kind(),
        ErrorKind::Undefined
    );
    for bad in [
        "{{ 1",
        "{% if %}",
        "{% if 1 %}",
        "{% for %}x{% endfor %}",
        "{% for x in y %}",
        "{% unknown %}",
        "{{ 'abc }}",
        "{{ 1 ! 2 }}",
        "{# open",
        "{% raw %}never",
        "{% macro %}{% endmacro %}",
        "{{ [1, 2 }}",
        "{{ a.[0] }}",
        "{{ f(a=1, 2) }}",
        "{{ x[1:2:3:4] }}",
        "{{ x[] }}",
        "{% set %}",
        "{% import 'a' %}",
        "{% from 'a' import %}",
        "{% include 'a' ignore %}",
        "{% with a %}{% endwith %}",
        "{{ 1 is }}",
        "{% for x in y %}{% else %}",
        "{{ 99999999999999999999 }}",
    ] {
        assert_eq!(
            Environment::new()
                .render_str(bad, &json!({}))
                .unwrap_err()
                .kind(),
            ErrorKind::Syntax,
            "{bad}"
        );
    }
    for bad in [
        "{{ 'a' + 1 }}",
        "{{ [1] < 'a' }}",
        "{{ 1 in 5 }}",
        "{{ 5|upper }}",
        "{{ 5|length }}",
        "{{ 1|keys }}",
        "{{ 1|dictsort }}",
        "{{ 'a'|abs }}",
        "{{ 2 ** 99999 }}",
        "{{ 9223372036854775807 * 9223372036854775807 * 9223372036854775807 }}",
        "{{ 1 // 0 }}",
        "{{ 1 % 0 }}",
        "{{ 1.0 // 0 }}",
        "{{ range(0, 1, 0) }}",
        "{{ range('a') }}",
        "{{ range(1, 2, 3, 4) }}",
        "{% for x in 5 %}{% endfor %}",
        "{% for a, b in [1] %}{% endfor %}",
        "{% set a, b = [1] %}",
        "{% set a, b = 1 %}",
        "{{ [1,2][1:2:0] }}",
        "{{ 5[1:2] }}",
        "{{ 5 is divisibleby 0 }}",
        "{{ 5 is eq }}",
        "{{ 'a'.nomethod() }}",
        "{{ [1]|map }}",
        "{{ [1]|selectattr }}",
        "{{ 'x'|int|upper }}",
        "{{ 1(2) }}",
        "{{ 1.0.attr() }}",
        "{{ m }}",
        "{{ [1]|sum(start='a') }}",
        "{{ '-'.join(5) }}",
        "{{ 'a'|replace('x', 5, 'z') }}",
    ] {
        let result = Environment::new().render_str(bad, &json!({"m": 1}));
        if bad == "{{ m }}" {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err(), "{bad} -> {result:?}");
        }
    }
    let mut env = Environment::new();
    env.add_template("m", "{% macro f() %}x{% endmacro %}")
        .unwrap();
    env.add_template("t", "{% import 'm' as m %}{{ m }}")
        .unwrap();
    assert!(env.render("t", &json!({})).is_err());
    env.add_template("t", "{% import 'm' as m %}{{ m.f }}")
        .unwrap();
    assert!(env.render("t", &json!({})).is_err());
    env.add_template("t", "{% import 'm' as m %}{{ m.f.x }}{{ m.f() }}")
        .unwrap();
    assert_eq!(env.render("t", &json!({})).unwrap(), "x");
    assert!(env.add_template("bad", "{% if %}").is_err());
}
