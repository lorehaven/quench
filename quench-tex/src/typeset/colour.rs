//! Colours as `\color` and `\textcolor` write them: xcolor's base names, `#rrggbb`, and the `HTML`, `rgb`, `RGB`
//! and `gray` models (`\color[HTML]{FF8000}`, `\color[rgb]{1,0.5,0}`).

use quench_raster::Color;

/// The colour `text` names, `None` when it is not one.
pub fn parse(text: &str) -> Option<Color> {
    let text = text.trim();
    if let Some((model, value)) = text.split_once(':') {
        return model_value(model.trim(), value.trim());
    }
    if text.starts_with('#') {
        return Color::from_hex(text);
    }
    let (r, g, b) = match text.to_ascii_lowercase().as_str() {
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "green" => (0, 255, 0),
        "blue" => (0, 0, 255),
        "cyan" => (0, 255, 255),
        "magenta" => (255, 0, 255),
        "yellow" => (255, 255, 0),
        "gray" | "grey" => (128, 128, 128),
        "darkgray" | "darkgrey" => (64, 64, 64),
        "lightgray" | "lightgrey" => (191, 191, 191),
        "brown" => (191, 128, 64),
        "lime" => (191, 255, 0),
        "olive" => (128, 128, 0),
        "orange" => (255, 128, 0),
        "pink" => (255, 191, 191),
        "purple" => (191, 0, 64),
        "teal" => (0, 128, 128),
        "violet" => (128, 0, 128),
        _ => return None,
    };
    Some(Color::rgb(r, g, b))
}

fn model_value(model: &str, value: &str) -> Option<Color> {
    let numbers = || {
        value
            .split(',')
            .map(|v| v.trim().parse::<f32>().ok())
            .collect::<Option<Vec<f32>>>()
    };
    let unit = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    match model {
        "HTML" | "html" => Color::from_hex(value).filter(|_| value.len() == 6),
        "rgb" => match numbers()?.as_slice() {
            [r, g, b] => Some(Color::rgb(unit(*r), unit(*g), unit(*b))),
            _ => None,
        },
        "RGB" => match numbers()?.as_slice() {
            [r, g, b] => {
                let byte = |v: f32| v.clamp(0.0, 255.0).round() as u8;
                Some(Color::rgb(byte(*r), byte(*g), byte(*b)))
            }
            _ => None,
        },
        "gray" => match numbers()?.as_slice() {
            [v] => Some(Color::rgb(unit(*v), unit(*v), unit(*v))),
            _ => None,
        },
        _ => None,
    }
}
