use quench_web::{Element, button};

#[derive(Clone, Copy, Debug)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Danger,
    Success,
    Warning,
    Outline,
}

impl ButtonVariant {
    #[must_use]
    pub const fn class_name(&self) -> &'static str {
        match self {
            Self::Primary => "btn btn-primary",
            Self::Secondary => "btn btn-secondary",
            Self::Danger => "btn btn-danger",
            Self::Success => "btn btn-success",
            Self::Warning => "btn btn-warning",
            Self::Outline => "btn btn-outline",
        }
    }
}

/// Builder for styled button elements with various variants
#[derive(Clone, Debug, Default)]
pub struct ButtonBuilder {
    text: String,
    variant: Option<ButtonVariant>,
    disabled: bool,
    button_type: String,
    id: Option<String>,
}

impl ButtonBuilder {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            button_type: "button".to_string(),
            ..Default::default()
        }
    }

    #[must_use]
    pub const fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = Some(variant);
        self
    }

    #[must_use]
    pub const fn primary(mut self) -> Self {
        self.variant = Some(ButtonVariant::Primary);
        self
    }

    #[must_use]
    pub const fn secondary(mut self) -> Self {
        self.variant = Some(ButtonVariant::Secondary);
        self
    }

    #[must_use]
    pub const fn danger(mut self) -> Self {
        self.variant = Some(ButtonVariant::Danger);
        self
    }

    #[must_use]
    pub const fn success(mut self) -> Self {
        self.variant = Some(ButtonVariant::Success);
        self
    }

    #[must_use]
    pub const fn warning(mut self) -> Self {
        self.variant = Some(ButtonVariant::Warning);
        self
    }

    #[must_use]
    pub const fn outline(mut self) -> Self {
        self.variant = Some(ButtonVariant::Outline);
        self
    }

    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    #[must_use]
    pub fn button_type(mut self, button_type: impl Into<String>) -> Self {
        self.button_type = button_type.into();
        self
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    #[must_use]
    pub fn build(self) -> Element {
        let mut btn = button().attr("type", &self.button_type).text(&self.text);

        if let Some(variant) = self.variant {
            btn = btn.class(variant.class_name());
        } else {
            btn = btn.class("btn");
        }

        if self.disabled {
            btn = btn.attr("disabled", "disabled");
        }

        if let Some(id) = self.id {
            btn = btn.attr("id", &id);
        }

        btn
    }
}

/// Helper functions for common button styles
pub fn primary_button(text: impl Into<String>) -> Element {
    ButtonBuilder::new(text).primary().build()
}

pub fn secondary_button(text: impl Into<String>) -> Element {
    ButtonBuilder::new(text).secondary().build()
}

pub fn danger_button(text: impl Into<String>) -> Element {
    ButtonBuilder::new(text).danger().build()
}

pub fn success_button(text: impl Into<String>) -> Element {
    ButtonBuilder::new(text).success().build()
}

pub fn warning_button(text: impl Into<String>) -> Element {
    ButtonBuilder::new(text).warning().build()
}

pub fn outline_button(text: impl Into<String>) -> Element {
    ButtonBuilder::new(text).outline().build()
}
