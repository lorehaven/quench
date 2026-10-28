//! Computed style: the cascade over a tree, and the typed values layout and painting read.

use std::collections::HashMap;
use std::rc::Rc;

use crate::color::{Color, ColorValue, parse_color};
use crate::length::{Length, Units, parse_length};
use crate::media::{MediaEnv, all_match};
use crate::selector::{Element, Specificity};
use crate::sheet::{Declaration, Rule, Stylesheet};
use crate::value::{Value, split_commas};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    None,
    Block,
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
    ListItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    None,
    Solid,
    Dashed,
    Dotted,
}

/// The size of one row or column of a grid.
#[derive(Debug, Clone, PartialEq)]
pub enum Track {
    Length(Length),
    /// A share of the space that is left: `1fr`.
    Fr(f32),
    /// As big as its content needs (`auto`, `min-content`, `max-content`), and stretched when there is room.
    Auto,
    /// At least the first, at most the second: `minmax(100px, 1fr)`.
    MinMax(Box<Track>, Box<Track>),
}

/// A `grid-template-columns` or `-rows`: the tracks, and the ones that `repeat(auto-fill, ...)` adds to fill the box.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrackList {
    pub tracks: Vec<Track>,
    /// Tracks that repeat after `tracks` as many times as fit; `true` collapses the empty ones (`auto-fit`).
    pub repeat_to_fill: Option<(Vec<Track>, bool)>,
}

/// One end of a grid item's place in a row or column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridLine {
    Auto,
    /// A line number from 1; negative numbers count from the end.
    Line(i32),
    /// As many tracks as this.
    Span(u32),
}

/// The curve of a transition: how the progress in time maps to progress in the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Timing {
    Linear,
    #[default]
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
}

/// One entry of `transition`: which property changes gradually, for how long, after what delay (seconds).
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// A property name, or `all`.
    pub property: String,
    pub duration: f32,
    pub delay: f32,
    pub timing: Timing,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            property: "all".into(),
            duration: 0.0,
            delay: 0.0,
            timing: Timing::Ease,
        }
    }
}

/// Which way an animation runs through its keyframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

/// What an animation does outside its run: before it starts and after it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillMode {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
}

/// One entry of `animation`: a `@keyframes` name played with a duration, delay, curve, count, direction and fill.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub name: String,
    pub duration: f32,
    pub delay: f32,
    pub timing: Timing,
    /// How many times it runs; `f32::INFINITY` for `infinite`.
    pub iterations: f32,
    pub direction: Direction,
    pub fill: FillMode,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            name: String::new(),
            duration: 0.0,
            delay: 0.0,
            timing: Timing::Ease,
            iterations: 1.0,
            direction: Direction::Normal,
            fill: FillMode::None,
        }
    }
}

/// `background-size`: the image's own size, or scaled to cover or fit inside the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundSize {
    Auto,
    Cover,
    Contain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    Visible,
    Hidden,
    Scroll,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Stretch,
    Start,
    End,
    Center,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Justify {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    End,
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextTransform {
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStyle {
    None,
    Disc,
    Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeight {
    Normal,
    /// A multiple of the font size.
    Number(f32),
    Px(f32),
}

/// One `box-shadow` layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: ColorValue,
    pub inset: bool,
}

/// `transform` and `rotate` folded into one: translations add, scales multiply, angles add.
#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    pub translate: (Length, Length),
    pub scale: (f32, f32),
    /// Degrees, clockwise.
    pub rotate: f32,
}

impl Transform {
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.translate.0.as_px() == Some(0.0)
            && self.translate.1.as_px() == Some(0.0)
            && self.scale == (1.0, 1.0)
            && self.rotate == 0.0
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translate: (Length::ZERO, Length::ZERO),
            scale: (1.0, 1.0),
            rotate: 0.0,
        }
    }
}

/// Everything layout and painting need to know about one element. Sides are `[top, right, bottom, left]`;
/// corners are `[top-left, top-right, bottom-right, bottom-left]`. `None` is `auto` (or no limit).
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    pub animations: Vec<Animation>,
    /// `grid-template-areas`: the name in each cell, row by row (`.` for an empty cell), and `grid-area` of an item.
    pub grid_areas: Vec<Vec<String>>,
    pub grid_area: Option<String>,
    pub transitions: Vec<Transition>,
    pub grid_columns: TrackList,
    pub grid_rows: TrackList,
    pub grid_auto_columns: Track,
    pub grid_auto_rows: Track,
    /// `grid-column` and `grid-row` of an item: where it starts and where it ends.
    pub grid_column: (GridLine, GridLine),
    pub grid_row: (GridLine, GridLine),
    /// The `url()` of `background-image`, how it is sized and placed, and whether it tiles.
    pub background_image: Option<String>,
    pub background_size: BackgroundSize,
    pub background_repeat: bool,
    /// Where the image sits in its box, as fractions of the free space: `(0.5, 0.5)` is `center`.
    pub background_position: (f32, f32),
    pub display: Display,
    pub position: Position,
    pub inset: [Option<Length>; 4],
    pub z_index: Option<i32>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    /// `None` is `auto`: zero for a block, the content's minimum for a flex item.
    pub min_width: Option<Length>,
    pub min_height: Option<Length>,
    pub max_width: Option<Length>,
    pub max_height: Option<Length>,
    pub border_box: bool,
    pub margin: [Option<Length>; 4],
    pub padding: [Length; 4],
    pub border_width: [f32; 4],
    pub border_style: [BorderStyle; 4],
    pub border_color: [ColorValue; 4],
    pub radius: [Length; 4],
    pub outline_width: f32,
    pub outline_style: BorderStyle,
    pub outline_color: ColorValue,
    pub outline_offset: f32,
    pub background: ColorValue,
    pub opacity: f32,
    pub shadows: Vec<Shadow>,
    pub overflow: (Overflow, Overflow),
    pub flex_direction: FlexDirection,
    pub flex_wrap: bool,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Option<Length>,
    pub align_items: Align,
    pub align_self: Option<Align>,
    pub justify_content: Justify,
    pub row_gap: Length,
    pub column_gap: Length,
    pub color: Color,
    pub font_family: Rc<Vec<String>>,
    pub font_size: f32,
    pub font_weight: u16,
    pub italic: bool,
    pub line_height: LineHeight,
    pub text_align: TextAlign,
    pub text_transform: TextTransform,
    pub list_style: ListStyle,
    pub visible: bool,
    pub transform: Transform,
    /// The custom properties in effect, resolved.
    pub custom: Rc<HashMap<String, Vec<Value>>>,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Inline,
            position: Position::Static,
            inset: [None, None, None, None],
            z_index: None,
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            border_box: false,
            margin: [
                Some(Length::ZERO),
                Some(Length::ZERO),
                Some(Length::ZERO),
                Some(Length::ZERO),
            ],
            padding: [Length::ZERO, Length::ZERO, Length::ZERO, Length::ZERO],
            border_width: [0.0; 4],
            border_style: [BorderStyle::None; 4],
            border_color: [ColorValue::CurrentColor; 4],
            radius: [Length::ZERO, Length::ZERO, Length::ZERO, Length::ZERO],
            outline_width: 3.0,
            outline_style: BorderStyle::None,
            outline_color: ColorValue::CurrentColor,
            outline_offset: 0.0,
            background: ColorValue::Color(Color::TRANSPARENT),
            animations: Vec::new(),
            transitions: Vec::new(),
            grid_areas: Vec::new(),
            grid_area: None,
            grid_columns: TrackList::default(),
            grid_rows: TrackList::default(),
            grid_auto_columns: Track::Auto,
            grid_auto_rows: Track::Auto,
            grid_column: (GridLine::Auto, GridLine::Auto),
            grid_row: (GridLine::Auto, GridLine::Auto),
            background_image: None,
            background_size: BackgroundSize::Auto,
            background_repeat: true,
            background_position: (0.0, 0.0),
            opacity: 1.0,
            shadows: Vec::new(),
            overflow: (Overflow::Visible, Overflow::Visible),
            flex_direction: FlexDirection::Row,
            flex_wrap: false,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: None,
            align_items: Align::Stretch,
            align_self: None,
            justify_content: Justify::Start,
            row_gap: Length::ZERO,
            column_gap: Length::ZERO,
            color: Color::BLACK,
            font_family: Rc::new(Vec::new()),
            font_size: 16.0,
            font_weight: 400,
            italic: false,
            line_height: LineHeight::Normal,
            text_align: TextAlign::Start,
            text_transform: TextTransform::None,
            list_style: ListStyle::None,
            visible: true,
            transform: Transform::default(),
            custom: Rc::new(HashMap::new()),
        }
    }
}

impl ComputedStyle {
    /// The style a child starts from: what inherits, and nothing else.
    #[must_use]
    pub fn inherited(&self) -> Self {
        Self {
            color: self.color,
            font_family: Rc::clone(&self.font_family),
            font_size: self.font_size,
            font_weight: self.font_weight,
            italic: self.italic,
            line_height: self.line_height,
            text_align: self.text_align,
            text_transform: self.text_transform,
            list_style: self.list_style,
            visible: self.visible,
            custom: Rc::clone(&self.custom),
            ..Self::default()
        }
    }

    /// A colour with `currentcolor` replaced by this style's `color`.
    #[must_use]
    pub const fn resolve(&self, color: ColorValue) -> Color {
        color.resolve(self.color)
    }

    /// The line height in pixels for this style's font size (`normal` is 1.36 of it, as Noto Sans reports).
    #[must_use]
    pub fn line_height_px(&self, normal: f32) -> f32 {
        match self.line_height {
            LineHeight::Normal => self.font_size * normal,
            LineHeight::Number(n) => self.font_size * n,
            LineHeight::Px(px) => px,
        }
    }
}

/// Style rules from one or more sheets, in the order they were added, ready to compute styles.
#[derive(Debug, Clone)]
pub struct Cascade {
    rules: Vec<Rule>,
    keyframes: HashMap<String, crate::sheet::Keyframes>,
    pub media: MediaEnv,
    /// `env(safe-area-inset-*)`: top, right, bottom, left, in device pixels.
    pub safe_area: [f32; 4],
    /// Device pixels per CSS pixel (see [`Units::zoom`]).
    pub zoom: f32,
}

impl Cascade {
    #[must_use]
    pub fn new(media: MediaEnv) -> Self {
        Self {
            rules: Vec::new(),
            keyframes: HashMap::new(),
            media,
            safe_area: [0.0; 4],
            zoom: 1.0,
        }
    }

    /// Adds a sheet; its rules come after (and win ties against) every sheet added before.
    pub fn add(&mut self, sheet: &Stylesheet) {
        let base = self.rules.len();
        for rule in &sheet.rules {
            let mut rule = rule.clone();
            rule.order += base;
            self.rules.push(rule);
        }
        for k in &sheet.keyframes {
            self.keyframes.insert(k.name.clone(), k.clone());
        }
    }

    /// The `@keyframes` of that name (the last one added wins).
    #[must_use]
    pub fn keyframes(&self, name: &str) -> Option<&crate::sheet::Keyframes> {
        self.keyframes.get(name)
    }

    /// `base` with the declarations of a keyframe applied: the style an element has at that point of an animation.
    #[must_use]
    pub fn style_with(
        &self,
        base: &ComputedStyle,
        parent: &ComputedStyle,
        root_font_size: f32,
        declarations: &[Declaration],
    ) -> ComputedStyle {
        let units = Units {
            font_size: base.font_size,
            root_font_size,
            viewport_width: self.media.width * self.zoom,
            viewport_height: self.media.height * self.zoom,
            zoom: self.zoom,
        };
        let mut style = base.clone();
        for d in declarations.iter().filter(|d| !d.name.starts_with("--")) {
            if let Some(values) = self.substitute(&d.value, &base.custom, 0) {
                apply(&mut style, parent, &d.name, &values, &units);
            }
        }
        style
    }

    #[must_use]
    pub const fn viewport(&self) -> (f32, f32) {
        (self.media.width, self.media.height)
    }

    /// The style of `element`, given its parent's (`None` for the root).
    #[must_use]
    pub fn compute<E: Element>(
        &self,
        element: &E,
        parent: Option<&ComputedStyle>,
        root_font_size: f32,
    ) -> ComputedStyle {
        self.compute_with(element, parent, root_font_size, &[])
    }

    /// Like [`compute`](Self::compute), with the declarations of the element's own `style` attribute, which win
    /// over every rule (the important ones among them over the important ones of the rules).
    #[must_use]
    pub fn compute_with<E: Element>(
        &self,
        element: &E,
        parent: Option<&ComputedStyle>,
        root_font_size: f32,
        inline: &[Declaration],
    ) -> ComputedStyle {
        let default = ComputedStyle {
            font_size: 16.0 * self.zoom,
            ..ComputedStyle::default()
        };
        let parent = parent.unwrap_or(&default);
        let mut style = parent.inherited();
        let mut winners: Vec<(bool, Specificity, usize, usize, &Declaration)> = Vec::new();
        for rule in &self.rules {
            if all_match(&rule.media, &self.media) && rule.selector.matches(element) {
                for (i, d) in rule.declarations.iter().enumerate() {
                    winners.push((d.important, rule.specificity, rule.order, i, d));
                }
            }
        }
        for (i, d) in inline.iter().enumerate() {
            winners.push((d.important, Specificity::MAX, usize::MAX, i, d));
        }
        winners.sort_by_key(|w| (w.0, w.1, w.2, w.3));

        // Custom properties first, each resolved where it is declared.
        let mut raw: HashMap<String, Vec<Value>> = (*parent.custom).clone();
        let own: Vec<&Declaration> = winners
            .iter()
            .map(|w| w.4)
            .filter(|d| d.name.starts_with("--"))
            .collect();
        for d in &own {
            raw.insert(d.name.clone(), d.value.clone());
        }
        if !own.is_empty() {
            let mut resolved = (*parent.custom).clone();
            for d in &own {
                if let Some(v) = self.substitute(&d.value, &raw, 0) {
                    resolved.insert(d.name.clone(), v);
                } else {
                    resolved.remove(&d.name);
                }
            }
            style.custom = Rc::new(resolved);
        }

        let parent_units = Units {
            font_size: parent.font_size,
            root_font_size,
            viewport_width: self.media.width * self.zoom,
            viewport_height: self.media.height * self.zoom,
            zoom: self.zoom,
        };
        let declarations: Vec<(&str, Vec<Value>)> = winners
            .iter()
            .filter(|w| !w.4.name.starts_with("--"))
            .filter_map(|w| {
                Some((
                    w.4.name.as_str(),
                    self.substitute(&w.4.value, &style.custom, 0)?,
                ))
            })
            .collect();
        // The font size first: `em` in every other declaration is relative to it, and `em` in the font size
        // itself is relative to the parent's.
        let mut scratch = style.clone();
        for (name, values) in declarations
            .iter()
            .filter(|(n, _)| matches!(*n, "font" | "font-size"))
        {
            apply(&mut scratch, parent, name, values, &parent_units);
        }
        let own_units = Units {
            font_size: scratch.font_size,
            ..parent_units
        };
        for (name, values) in &declarations {
            let units = if matches!(*name, "font" | "font-size") {
                &parent_units
            } else {
                &own_units
            };
            apply(&mut style, parent, name, values, units);
        }
        for i in 0..4 {
            if style.border_style[i] == BorderStyle::None {
                style.border_width[i] = 0.0;
            }
        }
        if style.outline_style == BorderStyle::None {
            style.outline_width = 0.0;
        }
        if style.position == Position::Absolute || style.position == Position::Fixed {
            // Out of flow boxes are blocks.
            style.display = match style.display {
                Display::Inline | Display::InlineBlock | Display::ListItem => Display::Block,
                Display::InlineFlex => Display::Flex,
                other => other,
            };
        }
        style
    }

    /// Replaces `var()` and `env()` in `values`; `None` when a variable has no value and no fallback.
    fn substitute(
        &self,
        values: &[Value],
        custom: &HashMap<String, Vec<Value>>,
        depth: u32,
    ) -> Option<Vec<Value>> {
        if depth > 16 {
            return None;
        }
        let mut out = Vec::with_capacity(values.len());
        for value in values {
            match value {
                Value::Function(name, args) if name == "var" => {
                    let Some(Value::Ident(var)) = args.first() else {
                        return None;
                    };
                    let fallback = args
                        .iter()
                        .position(|v| *v == Value::Comma)
                        .map(|i| &args[i + 1..]);
                    if let Some(found) = custom.get(var) {
                        out.extend(self.substitute(found, custom, depth + 1)?);
                    } else {
                        out.extend(self.substitute(fallback?, custom, depth + 1)?);
                    }
                }
                Value::Function(name, args) if name == "env" => {
                    let side = match args.first() {
                        Some(Value::Ident(n)) => match n.as_str() {
                            "safe-area-inset-top" => Some(0),
                            "safe-area-inset-right" => Some(1),
                            "safe-area-inset-bottom" => Some(2),
                            "safe-area-inset-left" => Some(3),
                            _ => None,
                        },
                        _ => None,
                    };
                    match (side, args.iter().position(|v| *v == Value::Comma)) {
                        (Some(i), _) => {
                            out.push(Value::Dimension(self.safe_area[i] / self.zoom, "px".into()));
                        }
                        (None, Some(i)) => {
                            out.extend(self.substitute(&args[i + 1..], custom, depth + 1)?);
                        }
                        (None, None) => out.push(Value::Dimension(0.0, "px".into())),
                    }
                }
                Value::Function(name, args) => out.push(Value::Function(
                    name.clone(),
                    self.substitute(args, custom, depth + 1)?,
                )),
                Value::Block(c, inner) => {
                    out.push(Value::Block(*c, self.substitute(inner, custom, depth + 1)?));
                }
                other => out.push(other.clone()),
            }
        }
        Some(out)
    }
}

fn iterations_of(v: &Value) -> Option<f32> {
    match v {
        Value::Ident(s) if s.eq_ignore_ascii_case("infinite") => Some(f32::INFINITY),
        Value::Number(n) if *n >= 0.0 => Some(*n),
        _ => None,
    }
}

fn direction_of(v: &Value) -> Option<Direction> {
    match v {
        Value::Ident(s) => match s.to_ascii_lowercase().as_str() {
            "normal" => Some(Direction::Normal),
            "reverse" => Some(Direction::Reverse),
            "alternate" => Some(Direction::Alternate),
            "alternate-reverse" => Some(Direction::AlternateReverse),
            _ => None,
        },
        _ => None,
    }
}

fn fill_of(v: &Value) -> Option<FillMode> {
    match v {
        Value::Ident(s) => match s.to_ascii_lowercase().as_str() {
            "none" => Some(FillMode::None),
            "forwards" => Some(FillMode::Forwards),
            "backwards" => Some(FillMode::Backwards),
            "both" => Some(FillMode::Both),
            _ => None,
        },
        _ => None,
    }
}

/// `animation: spin 1s linear infinite, fade 2s ease-in 0.5s 3 alternate both`.
fn animation_list(values: &[Value]) -> Option<Vec<Animation>> {
    let mut out = Vec::new();
    for part in split_commas(values) {
        if matches!(part.as_slice(), [Value::Ident(s)] if s.eq_ignore_ascii_case("none")) {
            continue;
        }
        let mut a = Animation::default();
        let mut times = 0;
        for v in &part {
            if let Some(seconds) = time_of(v).filter(|_| !matches!(v, Value::Number(_))) {
                if times == 0 {
                    a.duration = seconds.max(0.0);
                } else {
                    a.delay = seconds;
                }
                times += 1;
            } else if let Some(n) = iterations_of(v) {
                a.iterations = n;
            } else if let Some(d) = direction_of(v) {
                a.direction = d;
            } else if let Some(f) = fill_of(v) {
                a.fill = f;
            } else if let Some(t) = timing_of(v) {
                a.timing = t;
            } else if matches!(v, Value::Ident(s) if matches!(s.to_ascii_lowercase().as_str(), "running" | "paused"))
            {
            } else if let Value::Ident(s) = v {
                a.name.clone_from(s);
            } else {
                return None;
            }
        }
        out.push(a);
    }
    Some(out)
}

/// A time in seconds: `0.3s`, `200ms`.
fn time_of(v: &Value) -> Option<f32> {
    match v {
        Value::Dimension(n, unit) if unit.eq_ignore_ascii_case("s") => Some(*n),
        Value::Dimension(n, unit) if unit.eq_ignore_ascii_case("ms") => Some(*n / 1000.0),
        Value::Number(n) if *n == 0.0 => Some(0.0),
        _ => None,
    }
}

fn timing_of(v: &Value) -> Option<Timing> {
    match v {
        Value::Ident(s) => match s.to_ascii_lowercase().as_str() {
            "linear" | "step-start" | "step-end" => Some(Timing::Linear),
            "ease" => Some(Timing::Ease),
            "ease-in" => Some(Timing::EaseIn),
            "ease-out" => Some(Timing::EaseOut),
            "ease-in-out" => Some(Timing::EaseInOut),
            _ => None,
        },
        // Custom curves and steps are approximated by the nearest named one.
        Value::Function(name, _) if name.eq_ignore_ascii_case("cubic-bezier") => Some(Timing::Ease),
        Value::Function(name, _) if name.eq_ignore_ascii_case("steps") => Some(Timing::Linear),
        _ => None,
    }
}

/// Makes the transition list at least `n` long (new entries are `all 0s ease`).
fn fill_transitions(style: &mut ComputedStyle, n: usize) {
    while style.transitions.len() < n {
        style.transitions.push(Transition::default());
    }
}

/// `transition: opacity 0.3s ease-in-out 0.1s, color 1s`.
fn transition_list(values: &[Value]) -> Option<Vec<Transition>> {
    let mut out = Vec::new();
    for part in split_commas(values) {
        if matches!(part.as_slice(), [Value::Ident(s)] if s.eq_ignore_ascii_case("none")) {
            continue;
        }
        let mut t = Transition::default();
        let mut times = 0;
        for v in &part {
            if let Some(seconds) = time_of(v) {
                if times == 0 {
                    t.duration = seconds.max(0.0);
                } else {
                    t.delay = seconds;
                }
                times += 1;
            } else if let Some(curve) = timing_of(v) {
                t.timing = curve;
            } else if let Value::Ident(s) = v {
                t.property = s.to_ascii_lowercase();
            } else {
                return None;
            }
        }
        out.push(t);
    }
    Some(out)
}

/// One track: a length, `1fr`, `auto` or `minmax(a, b)`.
fn track_of(v: &Value, units: &Units) -> Option<Track> {
    match v {
        Value::Dimension(n, unit) if unit.eq_ignore_ascii_case("fr") => Some(Track::Fr(*n)),
        Value::Ident(s)
            if matches!(
                s.to_ascii_lowercase().as_str(),
                "auto" | "min-content" | "max-content"
            ) =>
        {
            Some(Track::Auto)
        }
        Value::Function(name, args) if name.eq_ignore_ascii_case("minmax") => {
            let parts = split_commas(args);
            let [a, b] = parts.as_slice() else {
                return None;
            };
            let (a, b) = (track_of(a.first()?, units)?, track_of(b.first()?, units)?);
            Some(Track::MinMax(Box::new(a), Box::new(b)))
        }
        other => parse_length(other, units).map(Track::Length),
    }
}

/// The value of `grid-template-columns` or `-rows`: tracks, line names (ignored) and `repeat()`.
fn track_list(values: &[Value], units: &Units) -> Option<TrackList> {
    let mut list = TrackList::default();
    for v in values {
        match v {
            Value::Ident(s) if s.eq_ignore_ascii_case("none") => {}
            Value::Block('[', _) => {}
            Value::Function(name, args) if name.eq_ignore_ascii_case("repeat") => {
                let parts = split_commas(args);
                let (head, tracks) = parts.split_first()?;
                let inner: Vec<Track> = tracks
                    .iter()
                    .flat_map(|part| part.iter())
                    .filter(|v| !matches!(v, Value::Block('[', _)))
                    .map(|v| track_of(v, units))
                    .collect::<Option<_>>()?;
                if inner.is_empty() {
                    return None;
                }
                match head.as_slice() {
                    [Value::Number(n)] if *n >= 1.0 => {
                        for _ in 0..(*n as usize).min(1000) {
                            list.tracks.extend(inner.iter().cloned());
                        }
                    }
                    [Value::Ident(s)] if s.eq_ignore_ascii_case("auto-fill") => {
                        list.repeat_to_fill = Some((inner, false));
                    }
                    [Value::Ident(s)] if s.eq_ignore_ascii_case("auto-fit") => {
                        list.repeat_to_fill = Some((inner, true));
                    }
                    _ => return None,
                }
            }
            other => list.tracks.push(track_of(other, units)?),
        }
    }
    Some(list)
}

/// One end of an item's place: `3`, `span 2`, `auto`.
fn grid_line(values: &[Value]) -> Option<GridLine> {
    match values {
        [Value::Ident(s)] if s.eq_ignore_ascii_case("auto") => Some(GridLine::Auto),
        [Value::Number(n)] if *n != 0.0 => Some(GridLine::Line(*n as i32)),
        [Value::Ident(s), Value::Number(n)] | [Value::Number(n), Value::Ident(s)]
            if s.eq_ignore_ascii_case("span") && *n >= 1.0 =>
        {
            Some(GridLine::Span(*n as u32))
        }
        _ => None,
    }
}

/// `grid-column` or `grid-row`: `1 / 3`, `2`, `span 2`, `1 / span 2`.
fn grid_lines(values: &[Value]) -> Option<(GridLine, GridLine)> {
    let mut parts = values.split(|v| matches!(v, Value::Delim('/')));
    let start = grid_line(parts.next()?)?;
    let end = match parts.next() {
        Some(part) => grid_line(part)?,
        None => GridLine::Auto,
    };
    parts.next().is_none().then_some((start, end))
}

/// The address in a `url(...)`.
fn url_of(v: &Value) -> Option<String> {
    match v {
        Value::Function(name, args) if name.eq_ignore_ascii_case("url") => match args.as_slice() {
            [Value::Str(s)] => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// A keyword of the `background` shorthand.
fn background_word(style: &mut ComputedStyle, v: &Value) {
    if let Value::Ident(s) = v {
        match s.to_ascii_lowercase().as_str() {
            "no-repeat" => style.background_repeat = false,
            "repeat" => style.background_repeat = true,
            "cover" => style.background_size = BackgroundSize::Cover,
            "contain" => style.background_size = BackgroundSize::Contain,
            "center" => style.background_position = (0.5, 0.5),
            _ => {}
        }
    }
}

fn ident(values: &[Value]) -> Option<String> {
    match values {
        [Value::Ident(s)] => Some(s.to_ascii_lowercase()),
        _ => None,
    }
}

fn is_auto(v: &Value) -> bool {
    matches!(v, Value::Ident(s) if s.eq_ignore_ascii_case("auto"))
}

fn is_ident(v: &Value, name: &str) -> bool {
    matches!(v, Value::Ident(s) if s.eq_ignore_ascii_case(name))
}

/// One to four values as top, right, bottom, left.
fn sides<T: Clone>(values: &[Value], f: impl Fn(&Value) -> Option<T>) -> Option<[T; 4]> {
    let v: Vec<T> = values.iter().map(f).collect::<Option<_>>()?;
    match v.as_slice() {
        [a] => Some([a.clone(), a.clone(), a.clone(), a.clone()]),
        [a, b] => Some([a.clone(), b.clone(), a.clone(), b.clone()]),
        [a, b, c] => Some([a.clone(), b.clone(), c.clone(), b.clone()]),
        [a, b, c, d] => Some([a.clone(), b.clone(), c.clone(), d.clone()]),
        _ => None,
    }
}

const fn side_of(s: &str) -> Option<usize> {
    match s.as_bytes() {
        b"top" => Some(0),
        b"right" => Some(1),
        b"bottom" => Some(2),
        b"left" => Some(3),
        _ => None,
    }
}

fn border_style_of(v: &Value) -> Option<BorderStyle> {
    let Value::Ident(s) = v else { return None };
    Some(match s.to_ascii_lowercase().as_str() {
        "none" | "hidden" => BorderStyle::None,
        "solid" | "double" | "groove" | "ridge" | "inset" | "outset" => BorderStyle::Solid,
        "dashed" => BorderStyle::Dashed,
        "dotted" => BorderStyle::Dotted,
        _ => return None,
    })
}

fn width_px(v: &Value, units: &Units) -> Option<f32> {
    if let Value::Ident(s) = v {
        return match s.to_ascii_lowercase().as_str() {
            "thin" => Some(units.zoom),
            "medium" => Some(3.0 * units.zoom),
            "thick" => Some(5.0 * units.zoom),
            _ => None,
        };
    }
    parse_length(v, units)?.as_px().map(|px| px.max(0.0))
}

/// `[width] [style] [color]` in any order.
fn line(
    values: &[Value],
    units: &Units,
) -> Option<(Option<f32>, Option<BorderStyle>, Option<ColorValue>)> {
    let (mut w, mut s, mut c) = (None, None, None);
    for v in values {
        if let Some(x) = width_px(v, units) {
            w = Some(x);
        } else if let Some(x) = border_style_of(v) {
            s = Some(x);
        } else {
            let x = parse_color(v)?;
            c = Some(x);
        }
    }
    Some((w, s, c))
}

fn angle(v: &Value) -> Option<f32> {
    match v {
        Value::Dimension(n, u) => match u.to_ascii_lowercase().as_str() {
            "deg" => Some(*n),
            "rad" => Some(n.to_degrees()),
            "turn" => Some(n * 360.0),
            "grad" => Some(n * 0.9),
            _ => None,
        },
        Value::Number(n) if *n == 0.0 => Some(0.0),
        _ => None,
    }
}

fn align_of(v: &str) -> Option<Align> {
    Some(match v {
        "stretch" | "normal" => Align::Stretch,
        "flex-start" | "start" | "self-start" | "left" => Align::Start,
        "flex-end" | "end" | "self-end" | "right" => Align::End,
        "center" => Align::Center,
        "baseline" => Align::Baseline,
        _ => return None,
    })
}

fn font_size_of(v: &Value, parent: f32, units: &Units) -> Option<f32> {
    match v {
        Value::Ident(s) => Some(match s.to_ascii_lowercase().as_str() {
            "xx-small" => 9.0 * units.zoom,
            "x-small" => 10.0 * units.zoom,
            "small" => 13.0 * units.zoom,
            "medium" => 16.0 * units.zoom,
            "large" => 18.0 * units.zoom,
            "x-large" => 24.0 * units.zoom,
            "xx-large" => 32.0 * units.zoom,
            "larger" => parent * 1.2,
            "smaller" => parent / 1.2,
            _ => return None,
        }),
        Value::Percent(p) => Some(parent * p / 100.0),
        other => parse_length(other, units)?.as_px().map(|px| px.max(0.0)),
    }
}

fn font_weight_of(v: &Value, parent: u16) -> Option<u16> {
    match v {
        Value::Number(n) if (1.0..=1000.0).contains(n) => Some(n.round() as u16),
        Value::Ident(s) => match s.to_ascii_lowercase().as_str() {
            "normal" => Some(400),
            "bold" => Some(700),
            "bolder" => Some(if parent < 400 {
                400
            } else if parent < 600 {
                700
            } else {
                900
            }),
            "lighter" => Some(if parent < 600 {
                100
            } else if parent < 800 {
                400
            } else {
                700
            }),
            _ => None,
        },
        _ => None,
    }
}

fn font_families(values: &[Value]) -> Vec<String> {
    split_commas(values)
        .iter()
        .filter_map(|part| match part.as_slice() {
            [Value::Str(s)] => Some(s.clone()),
            idents if !idents.is_empty() => {
                let words: Option<Vec<String>> = idents
                    .iter()
                    .map(|v| {
                        if let Value::Ident(s) = v {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                words.map(|w| w.join(" "))
            }
            _ => None,
        })
        .collect()
}

fn line_height_of(v: &Value, units: &Units) -> Option<LineHeight> {
    match v {
        Value::Ident(s) if s.eq_ignore_ascii_case("normal") => Some(LineHeight::Normal),
        Value::Number(n) => Some(LineHeight::Number(*n)),
        Value::Percent(p) => Some(LineHeight::Number(p / 100.0)),
        other => parse_length(other, units)?.as_px().map(LineHeight::Px),
    }
}

fn shadows_of(values: &[Value], units: &Units) -> Option<Vec<Shadow>> {
    if ident(values).as_deref() == Some("none") {
        return Some(Vec::new());
    }
    split_commas(values)
        .iter()
        .map(|part| {
            let mut numbers = Vec::new();
            let (mut inset, mut color) = (false, None);
            for v in part {
                if is_ident(v, "inset") {
                    inset = true;
                } else if let Some(c) = parse_color(v) {
                    color = Some(c);
                } else {
                    numbers.push(parse_length(v, units)?.as_px()?);
                }
            }
            if !(2..=4).contains(&numbers.len()) {
                return None;
            }
            Some(Shadow {
                x: numbers[0],
                y: numbers[1],
                blur: numbers.get(2).copied().unwrap_or(0.0).max(0.0),
                spread: numbers.get(3).copied().unwrap_or(0.0),
                color: color.unwrap_or(ColorValue::CurrentColor),
                inset,
            })
        })
        .collect()
}

fn transform_of(values: &[Value], units: &Units) -> Option<Transform> {
    let mut t = Transform::default();
    if ident(values).as_deref() == Some("none") {
        return Some(t);
    }
    let add = |a: &Length, b: Length| -> Length {
        match (a.as_px(), b.as_px()) {
            (Some(x), Some(y)) => Length::Px(x + y),
            _ => match (a, &b) {
                (Length::Percent(x), Length::Percent(y)) => Length::Percent(x + y),
                _ => b,
            },
        }
    };
    for v in values {
        let Value::Function(name, args) = v else {
            return None;
        };
        let args = split_commas(args);
        let len = |i: usize| {
            args.get(i)
                .and_then(|a| a.first())
                .and_then(|v| parse_length(v, units))
        };
        let num = |i: usize| args.get(i).and_then(|a| a.first()).and_then(Value::number);
        match name.to_ascii_lowercase().as_str() {
            "translate" => {
                t.translate.0 = add(&t.translate.0, len(0)?);
                t.translate.1 = add(&t.translate.1, len(1).unwrap_or(Length::ZERO));
            }
            "translatex" => t.translate.0 = add(&t.translate.0, len(0)?),
            "translatey" => t.translate.1 = add(&t.translate.1, len(0)?),
            "scale" => {
                let x = num(0)?;
                t.scale = (t.scale.0 * x, t.scale.1 * num(1).unwrap_or(x));
            }
            "scalex" => t.scale.0 *= num(0)?,
            "scaley" => t.scale.1 *= num(0)?,
            "rotate" => t.rotate += angle(args.first()?.first()?)?,
            _ => return None,
        }
    }
    Some(t)
}

fn flex_of(style: &mut ComputedStyle, values: &[Value], units: &Units) -> Option<()> {
    if let Some(word) = ident(values) {
        let (g, s, b) = match word.as_str() {
            "none" => (0.0, 0.0, None),
            "auto" => (1.0, 1.0, None),
            "initial" => (0.0, 1.0, None),
            _ => return None,
        };
        (style.flex_grow, style.flex_shrink, style.flex_basis) = (g, s, b);
        return Some(());
    }
    let (mut numbers, mut basis) = (Vec::new(), None);
    for v in values {
        match v {
            Value::Number(n) if numbers.len() < 2 && *n >= 0.0 => numbers.push(*n),
            other if is_auto(other) => basis = Some(None),
            other => basis = Some(Some(parse_length(other, units)?)),
        }
    }
    style.flex_grow = numbers.first().copied().unwrap_or(1.0);
    style.flex_shrink = numbers.get(1).copied().unwrap_or(1.0);
    style.flex_basis = basis.unwrap_or(Some(Length::ZERO));
    Some(())
}

/// Applies `font: [style] [weight] size[/line-height] family`.
fn font_shorthand(
    style: &mut ComputedStyle,
    parent: &ComputedStyle,
    values: &[Value],
    units: &Units,
) -> Option<()> {
    if ident(values).as_deref() == Some("inherit") {
        style.font_family = Rc::clone(&parent.font_family);
        style.font_size = parent.font_size;
        style.font_weight = parent.font_weight;
        style.italic = parent.italic;
        style.line_height = parent.line_height;
        return Some(());
    }
    let mut at = 0;
    let (mut italic, mut weight) = (false, 400);
    while let Some(v) = values.get(at) {
        if is_ident(v, "italic") || is_ident(v, "oblique") {
            italic = true;
        } else if is_ident(v, "normal") {
        } else if let Some(w) = font_weight_of(v, parent.font_weight)
            .filter(|_| !matches!(v, Value::Number(n) if *n < 1.0))
        {
            weight = w;
        } else {
            break;
        }
        at += 1;
    }
    let size = font_size_of(values.get(at)?, parent.font_size, units)?;
    at += 1;
    let mut line_height = LineHeight::Normal;
    if matches!(values.get(at), Some(Value::Delim('/'))) {
        line_height = line_height_of(
            values.get(at + 1)?,
            &Units {
                font_size: size,
                ..*units
            },
        )?;
        at += 2;
    }
    let families = font_families(&values[at..]);
    style.italic = italic;
    style.font_weight = weight;
    style.font_size = size;
    style.line_height = line_height;
    style.font_family = Rc::new(families);
    Some(())
}

/// Applies one declaration; a value that does not parse leaves the style as it was, as in a browser.
fn apply(
    style: &mut ComputedStyle,
    parent: &ComputedStyle,
    name: &str,
    values: &[Value],
    units: &Units,
) {
    let _ = try_apply(style, parent, name, values, units);
}

#[allow(clippy::too_many_lines, reason = "one arm per property")]
fn try_apply(
    style: &mut ComputedStyle,
    parent: &ComputedStyle,
    name: &str,
    values: &[Value],
    units: &Units,
) -> Option<()> {
    let word = ident(values);
    let inherit = word.as_deref() == Some("inherit");
    let len = |v: &Value| parse_length(v, units);
    let one = || {
        if values.len() == 1 {
            values.first()
        } else {
            None
        }
    };
    match name {
        "display" => {
            style.display = match word?.as_str() {
                "none" => Display::None,
                "inline" => Display::Inline,
                "inline-block" => Display::InlineBlock,
                "flex" => Display::Flex,
                "inline-flex" => Display::InlineFlex,
                "grid" => Display::Grid,
                "inline-grid" => Display::InlineGrid,
                "list-item" => Display::ListItem,
                _ => Display::Block,
            };
        }
        "position" => {
            style.position = match word?.as_str() {
                "relative" | "sticky" => Position::Relative,
                "absolute" => Position::Absolute,
                "fixed" => Position::Fixed,
                _ => Position::Static,
            };
        }
        "top" | "right" | "bottom" | "left" => {
            style.inset[side_of(name)?] = if is_auto(one()?) {
                None
            } else {
                Some(len(one()?)?)
            };
        }
        "inset" => {
            style.inset = sides(values, |v| {
                if is_auto(v) {
                    Some(None)
                } else {
                    len(v).map(Some)
                }
            })?;
        }
        "z-index" => {
            style.z_index = if is_auto(one()?) {
                None
            } else {
                Some(one()?.number()?.round() as i32)
            }
        }
        "width" => {
            style.width = if is_auto(one()?) {
                None
            } else {
                Some(len(one()?)?)
            }
        }
        "height" => {
            style.height = if is_auto(one()?) {
                None
            } else {
                Some(len(one()?)?)
            }
        }
        "min-width" => {
            style.min_width = if is_auto(one()?) {
                None
            } else {
                Some(len(one()?)?)
            }
        }
        "min-height" => {
            style.min_height = if is_auto(one()?) {
                None
            } else {
                Some(len(one()?)?)
            }
        }
        "max-width" => {
            style.max_width = if word.as_deref() == Some("none") {
                None
            } else {
                Some(len(one()?)?)
            }
        }
        "max-height" => {
            style.max_height = if word.as_deref() == Some("none") {
                None
            } else {
                Some(len(one()?)?)
            }
        }
        "box-sizing" => style.border_box = word? == "border-box",
        "margin" => {
            style.margin = sides(values, |v| {
                if is_auto(v) {
                    Some(None)
                } else {
                    len(v).map(Some)
                }
            })?;
        }
        "margin-top" | "margin-right" | "margin-bottom" | "margin-left" => {
            style.margin[side_of(&name[7..])?] = if is_auto(one()?) {
                None
            } else {
                Some(len(one()?)?)
            };
        }
        "padding" => {
            style.padding = sides(values, |v| {
                len(v).filter(|l| l.as_px().is_none_or(|p| p >= 0.0))
            })?;
        }
        "padding-top" | "padding-right" | "padding-bottom" | "padding-left" => {
            style.padding[side_of(&name[8..])?] = len(one()?)?;
        }
        "border-radius" => {
            let head: Vec<Value> = values
                .iter()
                .take_while(|v| **v != Value::Delim('/'))
                .cloned()
                .collect();
            let [tl, tr, br, bl] = sides(&head, len)?;
            style.radius = [tl, tr, br, bl];
        }
        "border-width" => {
            style.border_width = sides(values, |v| width_px(v, units))?;
        }
        "border-style" => style.border_style = sides(values, border_style_of)?,
        "border-color" => style.border_color = sides(values, parse_color)?,
        "border" => {
            let (w, s, c) = line(values, units)?;
            for i in 0..4 {
                style.border_width[i] = w.unwrap_or(3.0);
                style.border_style[i] = s.unwrap_or(BorderStyle::None);
                style.border_color[i] = c.unwrap_or(ColorValue::CurrentColor);
            }
        }
        n if n.starts_with("border-") => {
            let rest = &n[7..];
            if let Some(i) = side_of(rest) {
                let (w, s, c) = line(values, units)?;
                style.border_width[i] = w.unwrap_or(3.0);
                style.border_style[i] = s.unwrap_or(BorderStyle::None);
                style.border_color[i] = c.unwrap_or(ColorValue::CurrentColor);
            } else {
                let (side, prop) = rest.split_once('-')?;
                let i = side_of(side)?;
                match prop {
                    "width" => style.border_width[i] = width_px(one()?, units)?,
                    "style" => style.border_style[i] = border_style_of(one()?)?,
                    "color" => style.border_color[i] = parse_color(one()?)?,
                    _ => return None,
                }
            }
        }
        "outline" => {
            let (w, s, c) = line(values, units)?;
            style.outline_width = w.unwrap_or(3.0);
            style.outline_style = s.unwrap_or(BorderStyle::None);
            style.outline_color = c.unwrap_or(ColorValue::CurrentColor);
        }
        "outline-width" => style.outline_width = width_px(one()?, units)?,
        "outline-style" => style.outline_style = border_style_of(one()?)?,
        "outline-color" => style.outline_color = parse_color(one()?)?,
        "outline-offset" => style.outline_offset = len(one()?)?.as_px()?,
        "background" | "background-color" => {
            if word.as_deref() == Some("none") {
                style.background = ColorValue::Color(Color::TRANSPARENT);
            } else {
                let color = values.iter().rev().find_map(parse_color);
                style.background = color.unwrap_or(ColorValue::Color(Color::TRANSPARENT));
                if name == "background" {
                    // The shorthand sets the image layer too (gradients are not drawn).
                    style.background_image = values.iter().find_map(url_of);
                    style.background_size = BackgroundSize::Auto;
                    style.background_repeat = true;
                    style.background_position = (0.0, 0.0);
                    for v in values {
                        background_word(style, v);
                    }
                }
            }
        }
        "background-image" => style.background_image = values.iter().find_map(url_of),
        "background-size" => {
            style.background_size = match word.as_deref()? {
                "cover" => BackgroundSize::Cover,
                "contain" => BackgroundSize::Contain,
                "auto" => BackgroundSize::Auto,
                _ => return None,
            };
        }
        "background-repeat" => {
            style.background_repeat = match word.as_deref()? {
                "no-repeat" => false,
                "repeat" => true,
                _ => return None,
            };
        }
        "background-position" => {
            let mut position = (0.5, 0.5);
            for (n, v) in values.iter().enumerate() {
                match v {
                    Value::Ident(s)
                        if matches!(s.to_ascii_lowercase().as_str(), "top" | "bottom") =>
                    {
                        position.1 = if s.eq_ignore_ascii_case("top") {
                            0.0
                        } else {
                            1.0
                        };
                    }
                    Value::Ident(s)
                        if matches!(s.to_ascii_lowercase().as_str(), "left" | "right") =>
                    {
                        position.0 = if s.eq_ignore_ascii_case("left") {
                            0.0
                        } else {
                            1.0
                        };
                    }
                    Value::Ident(s) if s.eq_ignore_ascii_case("center") => {}
                    Value::Percent(p) => {
                        // The first percentage is horizontal, the second vertical.
                        if n == 0 {
                            position.0 = p / 100.0;
                        } else {
                            position.1 = p / 100.0;
                        }
                    }
                    _ => return None,
                }
            }
            style.background_position = position;
        }
        "opacity" => {
            style.opacity = match one()? {
                Value::Number(n) => n.clamp(0.0, 1.0),
                Value::Percent(p) => (p / 100.0).clamp(0.0, 1.0),
                _ => return None,
            };
        }
        "box-shadow" => style.shadows = shadows_of(values, units)?,
        "overflow" | "overflow-x" | "overflow-y" => {
            let of = |v: &Value| match v {
                Value::Ident(s) => Some(match s.to_ascii_lowercase().as_str() {
                    "visible" => Overflow::Visible,
                    "scroll" => Overflow::Scroll,
                    "auto" => Overflow::Auto,
                    "hidden" | "clip" => Overflow::Hidden,
                    _ => return None,
                }),
                _ => None,
            };
            match (name, values) {
                ("overflow", [a]) => style.overflow = (of(a)?, of(a)?),
                ("overflow", [a, b]) => style.overflow = (of(a)?, of(b)?),
                ("overflow-x", [a]) => style.overflow.0 = of(a)?,
                ("overflow-y", [a]) => style.overflow.1 = of(a)?,
                _ => return None,
            }
        }
        "flex" => flex_of(style, values, units)?,
        "flex-direction" => {
            style.flex_direction = match word?.as_str() {
                "row" => FlexDirection::Row,
                "row-reverse" => FlexDirection::RowReverse,
                "column" => FlexDirection::Column,
                "column-reverse" => FlexDirection::ColumnReverse,
                _ => return None,
            };
        }
        "flex-wrap" => style.flex_wrap = word? != "nowrap",
        "flex-grow" => style.flex_grow = one()?.number()?.max(0.0),
        "flex-shrink" => style.flex_shrink = one()?.number()?.max(0.0),
        "flex-basis" => {
            style.flex_basis = if is_auto(one()?) {
                None
            } else {
                Some(len(one()?)?)
            }
        }
        "align-items" => style.align_items = align_of(&word?)?,
        "align-self" => {
            style.align_self = if word.as_deref() == Some("auto") {
                None
            } else {
                Some(align_of(&word?)?)
            }
        }
        "justify-content" => {
            style.justify_content = match word?.as_str() {
                "flex-start" | "start" | "left" | "normal" => Justify::Start,
                "flex-end" | "end" | "right" => Justify::End,
                "center" => Justify::Center,
                "space-between" => Justify::SpaceBetween,
                "space-around" => Justify::SpaceAround,
                "space-evenly" => Justify::SpaceEvenly,
                _ => return None,
            };
        }
        "gap" | "grid-gap" => match values {
            [a] => (style.row_gap, style.column_gap) = (len(a)?, len(a)?),
            [a, b] => (style.row_gap, style.column_gap) = (len(a)?, len(b)?),
            _ => return None,
        },
        "animation" => style.animations = animation_list(values)?,
        "animation-name"
        | "animation-duration"
        | "animation-delay"
        | "animation-timing-function"
        | "animation-iteration-count"
        | "animation-direction"
        | "animation-fill-mode" => {
            let items = split_commas(values);
            while style.animations.len() < items.len() {
                style.animations.push(Animation::default());
            }
            for (i, a) in style.animations.iter_mut().enumerate() {
                let item = items[i % items.len()].as_slice();
                match name {
                    "animation-name" => {
                        a.name = match item {
                            [Value::Ident(s)] if s.eq_ignore_ascii_case("none") => String::new(),
                            [Value::Ident(s)] => s.clone(),
                            _ => return None,
                        };
                    }
                    "animation-duration" => a.duration = time_of(item.first()?)?.max(0.0),
                    "animation-delay" => a.delay = time_of(item.first()?)?,
                    "animation-timing-function" => a.timing = timing_of(item.first()?)?,
                    "animation-iteration-count" => a.iterations = iterations_of(item.first()?)?,
                    "animation-direction" => a.direction = direction_of(item.first()?)?,
                    _ => a.fill = fill_of(item.first()?)?,
                }
            }
        }
        "transition" => style.transitions = transition_list(values)?,
        "transition-property" => {
            let names = split_commas(values);
            fill_transitions(style, names.len());
            for (i, t) in style.transitions.iter_mut().enumerate() {
                t.property = match names[i % names.len()].as_slice() {
                    [Value::Ident(s)] => s.to_ascii_lowercase(),
                    _ => return None,
                };
            }
        }
        "transition-duration" | "transition-delay" => {
            let times = split_commas(values);
            let seconds: Vec<f32> = times
                .iter()
                .map(|t| match t.as_slice() {
                    [v] => time_of(v),
                    _ => None,
                })
                .collect::<Option<_>>()?;
            fill_transitions(style, seconds.len());
            for (i, t) in style.transitions.iter_mut().enumerate() {
                if name == "transition-duration" {
                    t.duration = seconds[i % seconds.len()].max(0.0);
                } else {
                    t.delay = seconds[i % seconds.len()];
                }
            }
        }
        "transition-timing-function" => {
            let curves = split_commas(values);
            fill_transitions(style, curves.len());
            for (i, t) in style.transitions.iter_mut().enumerate() {
                t.timing = timing_of(curves[i % curves.len()].first()?)?;
            }
        }
        "grid-template-columns" => style.grid_columns = track_list(values, units)?,
        "grid-template-rows" => style.grid_rows = track_list(values, units)?,
        "grid-template-areas" => {
            let mut rows: Vec<Vec<String>> = Vec::new();
            for v in values {
                match v {
                    Value::Ident(s) if s.eq_ignore_ascii_case("none") && values.len() == 1 => {}
                    Value::Str(s) => rows.push(s.split_whitespace().map(str::to_string).collect()),
                    _ => return None,
                }
            }
            // Every row has the same number of cells, and a name covers a rectangle.
            if rows
                .iter()
                .any(|r| r.len() != rows[0].len() || r.is_empty())
            {
                return None;
            }
            style.grid_areas = rows;
        }
        "grid-area" => match values {
            [Value::Ident(name)] if !name.eq_ignore_ascii_case("auto") => {
                style.grid_area = Some(name.clone());
            }
            _ => {
                // `row-start / column-start / row-end / column-end`, each from the end backwards when left out.
                let parts: Vec<&[Value]> =
                    values.split(|v| matches!(v, Value::Delim('/'))).collect();
                let lines: Vec<GridLine> =
                    parts.iter().map(|p| grid_line(p)).collect::<Option<_>>()?;
                let at = |i: usize| lines.get(i).copied().unwrap_or(GridLine::Auto);
                if lines.is_empty() || lines.len() > 4 {
                    return None;
                }
                style.grid_area = None;
                style.grid_row = (at(0), at(2));
                style.grid_column = (at(1), at(3));
            }
        },
        "grid-auto-columns" => style.grid_auto_columns = track_of(one()?, units)?,
        "grid-auto-rows" => style.grid_auto_rows = track_of(one()?, units)?,
        "grid-column" => style.grid_column = grid_lines(values)?,
        "grid-row" => style.grid_row = grid_lines(values)?,
        "grid-column-start" => style.grid_column.0 = grid_line(values)?,
        "grid-column-end" => style.grid_column.1 = grid_line(values)?,
        "grid-row-start" => style.grid_row.0 = grid_line(values)?,
        "grid-row-end" => style.grid_row.1 = grid_line(values)?,
        "row-gap" | "grid-row-gap" => style.row_gap = len(one()?)?,
        "column-gap" | "grid-column-gap" => style.column_gap = len(one()?)?,
        "color" => {
            if inherit {
                style.color = parent.color;
            } else {
                style.color = parse_color(one()?)?.resolve(parent.color);
            }
        }
        "font" => font_shorthand(style, parent, values, units)?,
        "font-family" => {
            if inherit {
                style.font_family = Rc::clone(&parent.font_family);
            } else {
                style.font_family = Rc::new(font_families(values));
            }
        }
        "font-size" => {
            style.font_size = if inherit {
                parent.font_size
            } else {
                font_size_of(one()?, parent.font_size, units)?
            };
        }
        "font-weight" => {
            style.font_weight = if inherit {
                parent.font_weight
            } else {
                font_weight_of(one()?, parent.font_weight)?
            };
        }
        "font-style" => style.italic = matches!(word?.as_str(), "italic" | "oblique"),
        "line-height" => {
            style.line_height = if inherit {
                parent.line_height
            } else {
                line_height_of(one()?, units)?
            }
        }
        "text-align" => {
            style.text_align = match word?.as_str() {
                "left" => TextAlign::Left,
                "right" => TextAlign::Right,
                "center" => TextAlign::Center,
                "end" => TextAlign::End,
                _ => TextAlign::Start,
            };
        }
        "text-transform" => {
            style.text_transform = match word?.as_str() {
                "uppercase" => TextTransform::Uppercase,
                "lowercase" => TextTransform::Lowercase,
                "capitalize" => TextTransform::Capitalize,
                _ => TextTransform::None,
            };
        }
        "list-style" => {
            style.list_style = if values.iter().any(|v| is_ident(v, "none")) {
                ListStyle::None
            } else if values.iter().any(|v| is_ident(v, "decimal")) {
                ListStyle::Decimal
            } else {
                ListStyle::Disc
            };
        }
        "visibility" => style.visible = !matches!(word?.as_str(), "hidden" | "collapse"),
        "transform" => {
            let t = transform_of(values, units)?;
            style.transform = t;
        }
        "rotate" => {
            if word.as_deref() == Some("none") {
                style.transform.rotate = 0.0;
            } else {
                style.transform.rotate = angle(one()?)?;
            }
        }
        _ => return None,
    }
    Some(())
}

/// Whether this engine accepts `name: value` (a value with `var()` or `env()` in it must be substituted first).
#[must_use]
pub fn accepts(name: &str, values: &[Value]) -> bool {
    let mut style = ComputedStyle::default();
    try_apply(
        &mut style,
        &ComputedStyle::default(),
        name,
        values,
        &Units::default(),
    )
    .is_some()
}
