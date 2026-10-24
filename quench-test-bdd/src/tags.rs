//! Tag expressions: `@fast and not @slow`, `@a or (@b and @c)`.

/// A parsed tag expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tags {
    Tag(String),
    Not(Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
}

fn normalize(tag: &str) -> &str {
    tag.strip_prefix('@').unwrap_or(tag)
}

fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    for c in text.chars() {
        match c {
            '(' | ')' => {
                if !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
                out.push(c.to_string());
            }
            c if c.is_whitespace() => {
                if !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

struct Parser {
    tokens: Vec<String>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.at).map(String::as_str)
    }

    fn or(&mut self) -> Result<Tags, String> {
        let mut left = self.and()?;
        while self.peek() == Some("or") {
            self.at += 1;
            left = Tags::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Tags, String> {
        let mut left = self.not()?;
        while self.peek() == Some("and") {
            self.at += 1;
            left = Tags::And(Box::new(left), Box::new(self.not()?));
        }
        Ok(left)
    }

    fn not(&mut self) -> Result<Tags, String> {
        if self.peek() == Some("not") {
            self.at += 1;
            return Ok(Tags::Not(Box::new(self.not()?)));
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Tags, String> {
        match self.peek() {
            None => Err("the tag expression ends early".to_string()),
            Some("(") => {
                self.at += 1;
                let inner = self.or()?;
                if self.peek() != Some(")") {
                    return Err("a `(` is never closed".to_string());
                }
                self.at += 1;
                Ok(inner)
            }
            Some(")" | "and" | "or") => Err(format!("unexpected `{}`", self.tokens[self.at])),
            Some(tag) => {
                let tag = normalize(tag).to_string();
                self.at += 1;
                Ok(Tags::Tag(tag))
            }
        }
    }
}

impl Tags {
    /// Parses an expression.
    ///
    /// # Errors
    ///
    /// A message when the expression is empty, unbalanced or has a misplaced operator.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut parser = Parser {
            tokens: tokens(text),
            at: 0,
        };
        let expression = parser.or()?;
        parser
            .peek()
            .map_or(Ok(expression), |extra| Err(format!("unexpected `{extra}`")))
    }

    /// Whether a scenario with `tags` (with or without `@`) satisfies the expression.
    #[must_use]
    pub fn eval(&self, tags: &[String]) -> bool {
        match self {
            Self::Tag(tag) => tags.iter().any(|t| normalize(t) == tag),
            Self::Not(inner) => !inner.eval(tags),
            Self::And(a, b) => a.eval(tags) && b.eval(tags),
            Self::Or(a, b) => a.eval(tags) || b.eval(tags),
        }
    }
}
