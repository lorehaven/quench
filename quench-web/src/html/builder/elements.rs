use crate::Element;

// Helper functions for common elements
#[must_use]
pub fn element(tag: &str) -> Element {
    Element::new(tag)
}

#[must_use]
pub fn button() -> Element {
    Element::new("button")
}

#[must_use]
pub fn div() -> Element {
    Element::new("div")
}

#[must_use]
pub fn strong() -> Element {
    Element::new("strong")
}

#[must_use]
pub fn header() -> Element {
    Element::new("header")
}

#[must_use]
pub fn content() -> Element {
    Element::new("content")
}

#[must_use]
pub fn footer() -> Element {
    Element::new("footer")
}

#[must_use]
pub fn nav() -> Element {
    Element::new("nav")
}

#[must_use]
pub fn a() -> Element {
    Element::new("a")
}

#[must_use]
pub fn h1() -> Element {
    Element::new("h1")
}

#[must_use]
pub fn h2() -> Element {
    Element::new("h2")
}

#[must_use]
pub fn h3() -> Element {
    Element::new("h3")
}

#[must_use]
pub fn p() -> Element {
    Element::new("p")
}

#[must_use]
pub fn pre() -> Element {
    Element::new("pre")
}

#[must_use]
pub fn span() -> Element {
    Element::new("span")
}

#[must_use]
pub fn form() -> Element {
    Element::new("form")
}

#[must_use]
pub fn label() -> Element {
    Element::new("label")
}

#[must_use]
pub fn ul() -> Element {
    Element::new("ul")
}

#[must_use]
pub fn li() -> Element {
    Element::new("li")
}

#[must_use]
pub fn i() -> Element {
    Element::new("i")
}

#[must_use]
pub fn input() -> Element {
    Element::new("input")
}

#[must_use]
pub fn textarea() -> Element {
    Element::new("textarea")
}

#[must_use]
pub fn select() -> Element {
    Element::new("select")
}

#[must_use]
pub fn option() -> Element {
    Element::new("option")
}

#[must_use]
pub fn checkbox() -> Element {
    Element::new("input").attr("type", "checkbox")
}

#[must_use]
pub fn meta() -> Element {
    Element::new("meta")
}

#[must_use]
pub fn style(content: String) -> Element {
    Element::new("style").text(content)
}

#[must_use]
pub fn script(content: String) -> Element {
    Element::new("script").text(content)
}
