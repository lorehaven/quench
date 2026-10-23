//! `rename_all` conventions.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameRule {
    Lower,
    Upper,
    Pascal,
    Camel,
    Snake,
    ScreamingSnake,
    Kebab,
    ScreamingKebab,
}

impl RenameRule {
    pub const ALL: [(&'static str, Self); 8] = [
        ("lowercase", Self::Lower),
        ("UPPERCASE", Self::Upper),
        ("PascalCase", Self::Pascal),
        ("camelCase", Self::Camel),
        ("snake_case", Self::Snake),
        ("SCREAMING_SNAKE_CASE", Self::ScreamingSnake),
        ("kebab-case", Self::Kebab),
        ("SCREAMING-KEBAB-CASE", Self::ScreamingKebab),
    ];

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, rule)| *rule)
    }

    /// Applies the rule to a field name written in `snake_case`.
    pub fn apply_to_field(self, field: &str) -> String {
        match self {
            Self::Lower | Self::Snake => field.to_string(),
            Self::Upper | Self::ScreamingSnake => field.to_uppercase(),
            Self::Pascal => field.split('_').map(capitalize).collect(),
            Self::Camel => {
                let pascal = Self::Pascal.apply_to_field(field);
                lower_first(&pascal)
            }
            Self::Kebab => field.replace('_', "-"),
            Self::ScreamingKebab => field.to_uppercase().replace('_', "-"),
        }
    }

    /// Applies the rule to a variant name written in `PascalCase`.
    pub fn apply_to_variant(self, variant: &str) -> String {
        match self {
            Self::Pascal => variant.to_string(),
            Self::Lower => variant.to_lowercase(),
            Self::Upper => variant.to_uppercase(),
            Self::Camel => lower_first(variant),
            Self::Snake => snake(variant),
            Self::ScreamingSnake => snake(variant).to_uppercase(),
            Self::Kebab => snake(variant).replace('_', "-"),
            Self::ScreamingKebab => snake(variant).to_uppercase().replace('_', "-"),
        }
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(chars).collect()
    })
}

fn snake(variant: &str) -> String {
    let mut out = String::with_capacity(variant.len() + 4);
    for (i, c) in variant.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_rule_name() {
        for (name, rule) in RenameRule::ALL {
            assert_eq!(RenameRule::parse(name), Some(rule));
        }
        assert_eq!(RenameRule::parse("Title Case"), None);
    }

    #[test]
    fn fields() {
        let field = "request_id_v2";
        let cases = [
            (RenameRule::Lower, "request_id_v2"),
            (RenameRule::Upper, "REQUEST_ID_V2"),
            (RenameRule::Pascal, "RequestIdV2"),
            (RenameRule::Camel, "requestIdV2"),
            (RenameRule::Snake, "request_id_v2"),
            (RenameRule::ScreamingSnake, "REQUEST_ID_V2"),
            (RenameRule::Kebab, "request-id-v2"),
            (RenameRule::ScreamingKebab, "REQUEST-ID-V2"),
        ];
        for (rule, expected) in cases {
            assert_eq!(rule.apply_to_field(field), expected, "{rule:?}");
        }
        assert_eq!(RenameRule::Pascal.apply_to_field(""), "");
        assert_eq!(RenameRule::Camel.apply_to_field(""), "");
    }

    #[test]
    fn variants() {
        let variant = "ServerErrorCode";
        let cases = [
            (RenameRule::Lower, "servererrorcode"),
            (RenameRule::Upper, "SERVERERRORCODE"),
            (RenameRule::Pascal, "ServerErrorCode"),
            (RenameRule::Camel, "serverErrorCode"),
            (RenameRule::Snake, "server_error_code"),
            (RenameRule::ScreamingSnake, "SERVER_ERROR_CODE"),
            (RenameRule::Kebab, "server-error-code"),
            (RenameRule::ScreamingKebab, "SERVER-ERROR-CODE"),
        ];
        for (rule, expected) in cases {
            assert_eq!(rule.apply_to_variant(variant), expected, "{rule:?}");
        }
    }
}
