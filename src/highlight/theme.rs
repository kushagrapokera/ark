//! Shared token-to-theme color mapping for viewer and config textarea.

use crate::render::{ColorRgb, Theme};

use super::Token;

pub fn token_color(theme: &Theme, token: Token) -> ColorRgb {
    match token {
        Token::Keyword => theme.syntax_keyword,
        Token::ControlKeyword => theme.syntax_control,
        Token::String => theme.syntax_string,
        Token::Number => theme.syntax_number,
        Token::Comment => theme.syntax_comment,
        Token::Type => theme.syntax_type,
        Token::Function => theme.syntax_function,
        Token::Constant => theme.syntax_constant,
        Token::Property => theme.syntax_property,
        Token::Variable => theme.syntax_variable,
        Token::Regex => theme.syntax_regex,
        Token::Escape => theme.syntax_escape,
        Token::Decorator => theme.syntax_decorator,
        Token::Selector => theme.syntax_selector,
    }
}
