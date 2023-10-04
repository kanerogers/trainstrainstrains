use common::yakui::{
    widgets::{Button, ButtonWidget, Pad, Text, TextWidget},
    Color, Response,
};

pub const HEART: &str = "\u{f004}";
pub const BOLT: &str = "\u{f0e7}";
pub const FORGE: &str = "\u{f06d}";
pub const FACTORY: &str = "\u{f275}";
pub const HOUSE: &str = "\u{f015}";
pub const HAMMER: &str = "\u{f6e3}";
pub const MOON: &str = "\u{f186}";

pub fn icon_text(font_size: f32, icon_codepoint: &'static str) -> Response<TextWidget> {
    let mut text = Text::new(font_size, icon_codepoint);
    text.style.font = "fontawesome".into();
    text.show()
}

pub fn icon_button(icon_codepoint: &'static str) -> Response<ButtonWidget> {
    let mut button = Button::unstyled(icon_codepoint);
    button.padding = Pad::all(4.0);
    button.style.text.font = "fontawesome".into();
    button.style.text.font_size = 20.0;
    button.style.fill = Color::GRAY;
    button.hover_style.text = button.style.text.clone();
    button.down_style.text = button.style.text.clone();
    button.hover_style.fill = Color::CORNFLOWER_BLUE;
    button.down_style.fill = button.hover_style.fill.adjust(0.7);
    button.show()
}
