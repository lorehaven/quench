use quench_web::{Element, div, h3};

/// Card component for displaying content in a styled container
#[derive(Clone, Debug, Default)]
pub struct Card {
    title: Option<String>,
    content: Option<Element>,
    footer: Option<Element>,
    compact: bool,
}

impl Card {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    #[must_use]
    pub fn content(mut self, content: Element) -> Self {
        self.content = Some(content);
        self
    }

    #[must_use]
    pub fn footer(mut self, footer: Element) -> Self {
        self.footer = Some(footer);
        self
    }

    #[must_use]
    pub const fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    #[must_use]
    pub fn build(self) -> Element {
        let mut card = div().class("card");

        if self.compact {
            card = card.class("card-compact");
        }

        if let Some(title) = self.title {
            card = card.child(div().class("card-header").child(h3().text(title)));
        }

        if let Some(content) = self.content {
            card = card.child(div().class("card-body").child(content));
        }

        if let Some(footer) = self.footer {
            card = card.child(div().class("card-footer").child(footer));
        }

        card
    }
}

/// Panel component for creating bordered/highlighted sections
#[derive(Clone, Debug, Default)]
pub struct Panel {
    title: Option<String>,
    content: Option<Element>,
    bordered: bool,
    highlighted: bool,
}

impl Panel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    #[must_use]
    pub fn content(mut self, content: Element) -> Self {
        self.content = Some(content);
        self
    }

    #[must_use]
    pub const fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }

    #[must_use]
    pub const fn highlighted(mut self, highlighted: bool) -> Self {
        self.highlighted = highlighted;
        self
    }

    #[must_use]
    pub fn build(self) -> Element {
        let mut panel = div().class("panel");

        if self.bordered {
            panel = panel.class("panel-bordered");
        }

        if self.highlighted {
            panel = panel.class("panel-highlighted");
        }

        if let Some(title) = self.title {
            panel = panel.child(div().class("panel-header").child(h3().text(title)));
        }

        if let Some(content) = self.content {
            panel = panel.child(div().class("panel-content").child(content));
        }

        panel
    }
}

/// Container for grouping content with optional styling
#[derive(Clone, Debug, Default)]
pub struct Container {
    content: Option<Element>,
    fluid: bool,
    centered: bool,
}

impl Container {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn content(mut self, content: Element) -> Self {
        self.content = Some(content);
        self
    }

    #[must_use]
    pub const fn fluid(mut self, fluid: bool) -> Self {
        self.fluid = fluid;
        self
    }

    #[must_use]
    pub const fn centered(mut self, centered: bool) -> Self {
        self.centered = centered;
        self
    }

    #[must_use]
    pub fn build(self) -> Element {
        let mut container = div().class("container");

        if self.fluid {
            container = container.class("container-fluid");
        }

        if self.centered {
            container = container.class("container-centered");
        }

        if let Some(content) = self.content {
            container = container.child(content);
        }

        container
    }
}

/// Helper functions for common containers
pub fn card(title: impl Into<String>, content: Element) -> Element {
    Card::new().title(title).content(content).build()
}

pub fn compact_card(title: impl Into<String>, content: Element) -> Element {
    Card::new()
        .title(title)
        .content(content)
        .compact(true)
        .build()
}

pub fn panel(title: impl Into<String>, content: Element) -> Element {
    Panel::new()
        .title(title)
        .content(content)
        .bordered(true)
        .build()
}

pub fn highlighted_panel(title: impl Into<String>, content: Element) -> Element {
    Panel::new()
        .title(title)
        .content(content)
        .bordered(true)
        .highlighted(true)
        .build()
}

/// A "nothing here" placeholder: `<div class="empty" data-i18n="{key}">`.
/// The surrounding content/container wrapper stays with the caller.
#[must_use]
pub fn empty_state(i18n_key: &str) -> Element {
    div().class("empty").attr("data-i18n", i18n_key)
}
