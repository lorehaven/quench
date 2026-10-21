use crate::{Element, footer, p};

#[derive(Clone, Debug, Default)]
pub struct FooterBuilder {
    label: String,
}

impl FooterBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn label(mut self, value: impl Into<String>) -> Self {
        self.label = value.into();
        self
    }

    #[must_use]
    pub fn build(self) -> Element {
        footer()
            .class("footer")
            .class("q-shell-footer")
            .child(p().attr("data-i18n", &self.label))
    }
}
