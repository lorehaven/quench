use crate::snapshot;
use quench_markdown_term::Options;

const CALLOUTS: &str = "{: .note }\nA note callout with enough text to wrap at forty cells wide.\n\n{: .warning }\n> In a quote.\n\n{: .important-title }\n> Heads up\n>\n> Titled callout.\n\nPlain after.\n";

/// just-the-docs callouts: a bar and a label; `-title` takes the label from the quote.
#[quench_test::test]
fn callouts() {
    snapshot(
        CALLOUTS,
        &Options {
            width: 40,
            ..Options::default()
        },
        r#"
┃ Note
┃ A note callout with enough text to
┃ wrap at forty cells wide.

┃ Warning
┃ In a quote.

┃ Heads up
┃ Titled callout.

Plain after.
"#,
    );
    snapshot(
        CALLOUTS,
        &Options {
            width: 80,
            ..Options::default()
        },
        r#"
┃ Note
┃ A note callout with enough text to wrap at forty cells wide.

┃ Warning
┃ In a quote.

┃ Heads up
┃ Titled callout.

Plain after.
"#,
    );
    snapshot(
        CALLOUTS,
        &Options {
            width: 120,
            ..Options::default()
        },
        r#"
┃ Note
┃ A note callout with enough text to wrap at forty cells wide.

┃ Warning
┃ In a quote.

┃ Heads up
┃ Titled callout.

Plain after.
"#,
    );
}

const LIQUID: &str = "# Page\n{: .no_toc }\n\n* TOC\n{:toc}\n\n{% include nav.html %}\n\n{% assign x = 1 %}\n\nSee [the index]({{ site.baseurl }}/index.html) and {{site.baseurl}}/raw.\n\n`{% raw %}{% include x %}{% endraw %}`\n";

/// Attribute lists and the TOC placeholder dropped, includes shown, other tags dropped, baseurl replaced.
#[quench_test::test]
fn liquid() {
    snapshot(
        LIQUID,
        &Options {
            width: 40,
            baseurl: "/book".to_owned(),
            ..Options::default()
        },
        r#"
Page
════════════════════════════════════════

[include: nav.html]

See the index [1] and /book/raw.

{% include x %}

────────────────────────────────────────
[1] /book/index.html
"#,
    );
    snapshot(
        LIQUID,
        &Options {
            width: 80,
            baseurl: "/book".to_owned(),
            ..Options::default()
        },
        r#"
Page
════════════════════════════════════════════════════════════════════════════════

[include: nav.html]

See the index [1] and /book/raw.

{% include x %}

────────────────────────────────────────────────────────────────────────────────
[1] /book/index.html
"#,
    );
    snapshot(
        LIQUID,
        &Options {
            width: 120,
            baseurl: "/book".to_owned(),
            ..Options::default()
        },
        r#"
Page
════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════

[include: nav.html]

See the index [1] and /book/raw.

{% include x %}

────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
[1] /book/index.html
"#,
    );
}

const RAW_BASEURL: &str = "Kept {% raw %}{{ site.baseurl }}{% endraw %}, replaced {{ site.baseurl }}/x.\n\n`{% raw %}{{ site.baseurl }}/y{% endraw %}`\n\n```\n{% raw %}{{ site.baseurl }}/z{% endraw %}\n{{ site.baseurl }}/w\n```\n";

/// Inside `{% raw %}` the baseurl tag is kept as written, as Jekyll does; outside it is replaced.
#[quench_test::test]
fn raw_keeps_baseurl() {
    snapshot(
        RAW_BASEURL,
        &Options {
            width: 60,
            baseurl: "/book".to_owned(),
            ..Options::default()
        },
        r#"
Kept {{ site.baseurl }}, replaced /book/x.

{{ site.baseurl }}/y

╭──────────────────────────────────────────────────────────╮
│ {{ site.baseurl }}/z                                     │
│ /book/w                                                  │
╰──────────────────────────────────────────────────────────╯
"#,
    );
    assert_eq!(
        quench_markdown_term::replace_baseurl(
            "{{ site.baseurl }} {%- raw -%}{{site.baseurl}}{%- endraw -%} {{site.baseurl}} {% raw %}{{ site.baseurl }}",
            "/b"
        ),
        "/b {%- raw -%}{{site.baseurl}}{%- endraw -%} /b {% raw %}{{ site.baseurl }}"
    );
}
