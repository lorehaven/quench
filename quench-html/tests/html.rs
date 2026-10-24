//! The HTML parser, the document tree and CSS selectors.

#![cfg(feature = "parse")]
#![allow(clippy::too_many_lines)]

use quench_html::{Document, NodeKind, Selector, decode_entities};

fn texts(doc: &Document, css: &str) -> Vec<String> {
    let selector = Selector::parse(css).unwrap_or_else(|e| panic!("{css}: {e}"));
    doc.select(&selector).map(|n| n.text()).collect()
}

#[test]
fn builds_html_head_and_body() {
    let doc = Document::parse("<!DOCTYPE html><title>T</title><p>one<p>two");
    let html = doc.root_element();
    assert_eq!(html.name(), Some("html"));
    let kids: Vec<_> = html.children().filter_map(|c| c.name()).collect();
    assert_eq!(kids, ["head", "body"]);
    assert_eq!(texts(&doc, "head > title"), ["T"]);
    assert_eq!(texts(&doc, "body > p"), ["one", "two"]);
    assert_eq!(
        doc.root().children().next().unwrap().kind(),
        NodeKind::Doctype
    );

    // An empty document still has the skeleton.
    let empty = Document::parse("");
    assert_eq!(empty.root_element().children().count(), 2);
    // Explicit tags and attributes merge into it.
    let doc = Document::parse(
        "<html lang=en><head><meta charset=utf-8></head><body class='a b' id=x>hi</body></html>",
    );
    assert_eq!(doc.root_element().attr("lang"), Some("en"));
    let body = doc
        .select(&Selector::parse("body").unwrap())
        .next()
        .unwrap();
    assert_eq!(
        (body.id(), body.classes().collect::<Vec<_>>()),
        (Some("x"), vec!["a", "b"])
    );
    assert!(body.has_class("b") && !body.has_class("c"));
    assert_eq!(texts(&doc, "meta[charset]"), [""]);
    // Text before any tag starts the body; whitespace before the head is dropped.
    let doc = Document::parse("  \n hello <b>world</b>");
    assert_eq!(texts(&doc, "body"), ["hello world"]);
    // A second <html>/<head>/<body> only adds missing attributes.
    let doc = Document::parse("<html a=1><html a=2 b=3><head></head><body x=1><body x=2 y=3>t");
    assert_eq!(
        doc.root_element().attrs().collect::<Vec<_>>(),
        [("a", "1"), ("b", "3")]
    );
    // Content after </body> still lands in the body.
    let doc = Document::parse("<body>a</body>b</html>c");
    assert_eq!(texts(&doc, "body"), ["abc"]);
}

#[test]
fn implicit_end_tags() {
    let doc = Document::parse("<ul><li>a<li>b<ul><li>c</ul><li>d</ul><dl><dt>t<dd>d<dt>t2</dl>");
    assert_eq!(texts(&doc, "ul > li"), ["a", "bc", "c", "d"]);
    assert_eq!(texts(&doc, "dt, dd"), ["t", "d", "t2"]);
    let doc = Document::parse("<table><tr><td>1<td>2<tr><th>3<td>4</table>");
    assert_eq!(texts(&doc, "tbody > tr"), ["12", "34"]);
    assert_eq!(texts(&doc, "tr:first-child td"), ["1", "2"]);
    let doc = Document::parse("<table><thead><tr><th>h<tbody><tr><td>b<tfoot><tr><td>f</table>");
    assert_eq!(texts(&doc, "thead th, tbody td, tfoot td"), ["h", "b", "f"]);
    let doc = Document::parse(
        "<p>a<div>b</div><p>c<h1>d<h2>e<select><option>x<option>y<optgroup><option>z</select><a href=1>p<a href=2>q<button>b1<button>b2",
    );
    assert_eq!(texts(&doc, "p"), ["a", "c"]);
    assert_eq!(texts(&doc, "h1"), ["d"]);
    assert!(texts(&doc, "h2")[0].starts_with('e'));
    assert_eq!(texts(&doc, "option"), ["x", "y", "z"]);
    assert_eq!(texts(&doc, "a"), ["p", "qb1b2"]);
    assert_eq!(texts(&doc, "button"), ["b1", "b2"]);
    // A stray end tag is ignored, a stray </p> makes a paragraph, </br> is a <br>.
    let doc = Document::parse("<div>a</span>b</p>c</br>d</div>");
    assert_eq!(texts(&doc, "div"), ["abcd"]);
    assert_eq!(texts(&doc, "p"), [""]);
    assert_eq!(texts(&doc, "br"), [""]);
    // End tags do not reach out of a table cell.
    let doc = Document::parse("<div><table><tr><td>x</div>y</td></tr></table></div>");
    assert_eq!(texts(&doc, "td"), ["xy"]);
}

#[test]
fn raw_text_comments_and_entities() {
    let doc = Document::parse(
        "<script>if (a < b && c) { x = '</p>'; }</script><style>p > a { }</style><textarea>\n<b>&amp;</textarea><title>a &lt; b</title><p>x",
    );
    assert_eq!(texts(&doc, "script"), ["if (a < b && c) { x = '</p>'; }"]);
    assert_eq!(texts(&doc, "style"), ["p > a { }"]);
    assert_eq!(texts(&doc, "textarea"), ["<b>&"]);
    assert_eq!(texts(&doc, "title"), ["a < b"]);
    assert_eq!(texts(&doc, "p"), ["x"]);
    let doc = Document::parse("<noscript><p>hidden</p></noscript><iframe><b>x</iframe><p>ok");
    assert_eq!(texts(&doc, "noscript"), ["<p>hidden</p>"]);
    assert_eq!(texts(&doc, "iframe"), ["<b>x"]);
    let doc = Document::parse("<!-- c1 --><p>a<!-- c2 -->b<!--><?php x ?><!bogus>c</p><!-->");
    let comments: Vec<_> = doc
        .root()
        .descendants()
        .filter(quench_html::Node::is_comment)
        .count()
        .to_string()
        .chars()
        .collect();
    assert_eq!(comments.iter().collect::<String>(), "6");
    assert_eq!(texts(&doc, "p"), ["abc"]);
    assert_eq!(
        decode_entities("&lt;&amp;&gt; &copy; &#65;&#x42; &nbsp;| &notit; &bogus; &amp"),
        "<&> © AB \u{a0}| ¬it; &bogus; &"
    );
    let doc = Document::parse("<a title=\"x &amp; y &copy=1 &copy; &#33;\" href='a?b=1&c=2'>t</a>");
    let a = doc.select(&Selector::parse("a").unwrap()).next().unwrap();
    assert_eq!(a.attr("title"), Some("x & y &copy=1 © !"));
    assert_eq!(a.attr("href"), Some("a?b=1&c=2"));
    // Text with a literal `<` and `&`.
    let doc = Document::parse("<p>1 < 2 && 3 <> 4 <3</p>");
    assert_eq!(texts(&doc, "p"), ["1 < 2 && 3 <> 4 <3"]);
}

#[test]
fn attributes_and_tags_are_read_loosely() {
    let doc = Document::parse(
        "<DIV ID=Main CLASS='x  y' data-a = \"1\" disabled DATA-A=2 / lang=en><IMG SRC=a.png/><br/><input value=a/b>text</DIV>",
    );
    let div = doc.select(&Selector::parse("div").unwrap()).next().unwrap();
    assert_eq!(div.id(), Some("Main"));
    assert_eq!(div.attr("data-a"), Some("1"));
    assert_eq!(div.attr("disabled"), Some(""));
    assert_eq!(div.attr("lang"), Some("en"));
    assert_eq!(div.attrs().count(), 5);
    assert_eq!(
        doc.select(&Selector::parse("img").unwrap())
            .next()
            .unwrap()
            .attr("src"),
        Some("a.png/")
    );
    assert_eq!(texts(&doc, "input"), [""]);
    assert_eq!(
        doc.select(&Selector::parse("input").unwrap())
            .next()
            .unwrap()
            .attr("value"),
        Some("a/b")
    );
    // Unterminated constructs end at the end of the input.
    for broken in [
        "<div class=\"a",
        "<div",
        "<!-- never",
        "<a href='x",
        "</",
        "<p><",
        "<p attr=",
        "<p =x>",
    ] {
        let _ = Document::parse(broken);
    }
}

#[test]
fn tree_navigation() {
    let doc = Document::parse("<div id=a><p>1</p><p>2<b>x</b></p><span>3</span></div>");
    let div = doc.select(&Selector::parse("#a").unwrap()).next().unwrap();
    assert_eq!(div.children().count(), 3);
    assert_eq!(div.children().next_back().unwrap().name(), Some("span"));
    assert_eq!(div.descendants().count(), 9);
    let b = doc.select(&Selector::parse("b").unwrap()).next().unwrap();
    let up: Vec<_> = b.ancestors().filter_map(|n| n.name()).collect();
    assert_eq!(up, ["p", "div", "body", "html"]);
    assert!(b.ancestors().last().unwrap().is_document());
    assert_eq!(b.parent().unwrap().name(), Some("p"));
    assert_eq!(b.text_nodes().collect::<Vec<_>>(), ["x"]);
    assert_eq!(b.prev_element(), None);
    assert_eq!(
        b.parent().unwrap().next_element().unwrap().name(),
        Some("span")
    );
    assert_eq!(b.parent().unwrap().prev_element().unwrap().text(), "1");
    assert_eq!(
        div.html(),
        "<div id=\"a\"><p>1</p><p>2<b>x</b></p><span>3</span></div>"
    );
    let doc = Document::parse(
        "<!DOCTYPE html><!--c--><p title='a\"b'>x &lt; y<br><script>a<b</script></p>",
    );
    assert_eq!(
        doc.root().html(),
        "<!DOCTYPE html><!--c--><html><head></head><body><p title=\"a&quot;b\">x &lt; y<br><script>a<b</script></p></body></html>"
    );
    assert_eq!(div.inner_html(), "<p>1</p><p>2<b>x</b></p><span>3</span>");
    assert_eq!(div.as_text(), None);
    assert_eq!(div.kind(), NodeKind::Element);
    assert_eq!(doc.root().kind(), NodeKind::Document);
    let t = b.children().next().unwrap();
    assert_eq!(
        (t.kind(), t.as_text(), t.name(), t.attr("x")),
        (NodeKind::Text, Some("x"), None, None)
    );
    assert_eq!(div, div);
    assert_ne!(div, b);
}

#[test]
fn selectors() {
    let doc = Document::parse(
        "<div id=main class='box wide'><ul class=list><li class=a>1<li class=b>2<li class='a c'>3<li>4</ul>\
         <p lang='en-US'>x<a href='http://e.com/x.pdf'>L</a><a href='/rel'>R</a><a>none</a></p>\
         <p>y</p><span></span><span><!-- c --></span><span> </span></div><h2>h</h2><table><tr><td>1<td>2<tr><td>3<td>4</table>",
    );
    let cases: &[(&str, &[&str])] = &[
        ("li", &["1", "2", "3", "4"]),
        ("LI", &["1", "2", "3", "4"]),
        ("*", &[]),
        ("#main > ul > li.a", &["1", "3"]),
        (".box.wide .list", &["1234"]),
        ("div .c", &["3"]),
        ("li.a.c", &["3"]),
        ("li:first-child", &["1"]),
        ("li:last-child", &["4"]),
        ("li:nth-child(2)", &["2"]),
        ("li:nth-child(odd)", &["1", "3"]),
        ("li:nth-child(even)", &["2", "4"]),
        ("li:nth-child(2n+1)", &["1", "3"]),
        ("li:nth-child(-n+2)", &["1", "2"]),
        ("li:nth-child(n+3)", &["3", "4"]),
        ("li:nth-last-child(1)", &["4"]),
        ("li:nth-of-type(2)", &["2"]),
        ("li:nth-last-of-type(2)", &["3"]),
        ("li:first-of-type", &["1"]),
        ("li:last-of-type", &["4"]),
        ("li:only-child", &[]),
        ("h2:only-of-type", &["h"]),
        ("li:not(.a)", &["2", "4"]),
        ("li:not(.a, .b)", &["4"]),
        ("li:is(.b, .c)", &["2", "3"]),
        (":where(h2)", &["h"]),
        ("li + li", &["2", "3", "4"]),
        ("li.a ~ li", &["2", "3", "4"]),
        ("li.b + li.a", &["3"]),
        ("a[href]", &["L", "R"]),
        ("a[href='/rel']", &["R"]),
        ("a[href=\"/rel\"]", &["R"]),
        ("a[href^='http']", &["L"]),
        ("a[href$='.pdf']", &["L"]),
        ("a[href*='e.com']", &["L"]),
        ("a[href~='x']", &[]),
        ("[class~=wide]", &["1234xLRnoney "]),
        ("p[lang|=en]", &["xLRnone"]),
        ("a[HREF='/REL' i]", &["R"]),
        ("a:not([href])", &["none"]),
        ("span:empty", &["", ""]),
        ("html:root, :root > body", &[]),
        ("ul li, h2", &["1", "2", "3", "4", "h"]),
        ("div > p, div > ul", &["1234", "xLRnone", "y"]),
        ("table td:nth-child(1)", &["1", "3"]),
        ("tr:nth-child(2) td", &["3", "4"]),
        ("  div   >   ul  ", &["1234"]),
        ("a:first-of-type + a", &["R"]),
    ];
    for (css, want) in cases {
        let got = texts(&doc, css);
        if want.is_empty() && *css == "*" {
            assert!(got.len() > 20);
        } else if *css == "html:root, :root > body" {
            assert_eq!(got.len(), 2, "{css}");
        } else {
            assert_eq!(got, *want, "{css}");
        }
    }
    // select on a node excludes itself.
    let div = doc.select(&Selector::parse("div").unwrap()).next().unwrap();
    assert_eq!(div.select(&Selector::parse("div, p").unwrap()).count(), 2);
    let sel = Selector::parse("p").unwrap();
    assert!(sel.matches(div.select(&sel).next().unwrap()));
    assert!(!sel.matches(div));
    assert!(!sel.matches(doc.root()));
}

#[test]
fn selector_errors() {
    for bad in [
        "",
        ",",
        "div,",
        "> p",
        "div >",
        "[",
        "[a",
        "[a=]",
        "[a~b]",
        "[a='x]",
        "li:nth-child(",
        "li:nth-child(x)",
        "li:nth-child(2n+)",
        ":nope",
        "li:not",
        "li:not(",
        "div..a",
        "#",
        "div )",
        "a[href^]",
        "p:nth-child",
    ] {
        let err = Selector::parse(bad);
        assert!(err.is_err(), "{bad:?} parsed");
    }
    assert!(
        Selector::parse(":nope")
            .unwrap_err()
            .to_string()
            .contains("unsupported pseudo-class")
    );
    // Escapes and unusual names are fine.
    assert!(Selector::parse("a\\:b, .x\\.y, [data-x='1'], é").is_ok());
}
