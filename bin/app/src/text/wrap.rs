/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use super::{Glyph, GlyphPositionIter};

#[derive(Debug, PartialEq)]
#[repr(u8)]
enum TokenType {
    Null,
    Word,
    Whitespace,
}

struct Token {
    token_type: TokenType,
    lhs: f32,
    rhs: f32,
    glyphs: Vec<Glyph>,
}

impl Token {
    #[allow(dead_code)]
    fn as_str(&self) -> String {
        glyph_str(&self.glyphs)
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.token_type {
            TokenType::Null => write!(f, "Token::Null")?,
            TokenType::Word => write!(f, "Token::Word")?,
            TokenType::Whitespace => write!(f, "Token::Whitespace")?,
        }
        write!(f, "({})", self.as_str())
    }
}

/// Get the string represented by a vec of glyphs. Useful for debugging.
pub fn glyph_str(glyphs: &[Glyph]) -> String {
    glyphs.iter().map(|g| g.substr.as_str()).collect::<Vec<_>>().join("")
}

fn tokenize(font_size: f32, window_scale: f32, glyphs: &Vec<Glyph>) -> Vec<Token> {
    let glyph_pos_iter = GlyphPositionIter::new(font_size, window_scale, glyphs, 0.);

    let mut tokens = vec![];
    let mut token_glyphs = vec![];
    let mut lhs = -1.;
    let mut rhs = 0.;

    let mut token_type = TokenType::Null;

    for (pos, glyph) in glyph_pos_iter.zip(glyphs.iter()) {
        let new_type = if glyph.substr.chars().all(char::is_whitespace) {
            TokenType::Whitespace
        } else {
            TokenType::Word
        };

        // This is the initial token so lets begin
        // Just assume the token_type
        if token_type == TokenType::Null {
            assert!(token_glyphs.is_empty());
            token_type = new_type;
        } else if new_type != token_type {
            // We just changed from one token type to another
            assert!(!token_glyphs.is_empty());

            // We have a non-empty word to push
            let token = Token { token_type, lhs, rhs, glyphs: std::mem::take(&mut token_glyphs) };
            tokens.push(token);

            // Reset ruler
            lhs = -1.;
            //rhs = 0.;
            // take() blanked token_glyphs above

            token_type = new_type;
        }

        // LHS is uninitialized so this is the first glyph in the word
        if lhs < 0. {
            lhs = pos.x;
        }

        // RHS should always be the max
        rhs = pos.x + pos.w;

        // Update word
        token_glyphs.push(glyph.clone());
    }

    if !token_glyphs.is_empty() {
        let token = Token { token_type, lhs, rhs, glyphs: std::mem::take(&mut token_glyphs) };
        tokens.push(token);
    }

    tokens
}

/// Given a series of words, apply wrapping.
/// Whitespace is completely perserved.
fn apply_wrap(line_width: f32, mut tokens: Vec<Token>) -> Vec<Vec<Glyph>> {
    //debug!(target: "text::wrap", "apply_wrap({line_width}, {tokens:?})");

    let mut lines = vec![];
    let mut line = vec![];
    let mut start = 0.;

    let mut tokens_iter = tokens.iter_mut().peekable();
    while let Some(token) = tokens_iter.next() {
        assert!(token.token_type != TokenType::Null);

        // Does this token cross over the end of the line?
        if token.rhs > start + line_width {
            // Whitespace tokens that cause wrapping are prepended to the current line before
            // making the line break.
            if token.token_type == TokenType::Whitespace {
                line.append(&mut token.glyphs);
            }

            // Start a new line
            let line = std::mem::take(&mut line);
            //debug!(target: "text::apply_wrap", "adding line: {}", glyph_str(&line));
            // This can happen if this token is very long and crosses the line boundary
            if !line.is_empty() {
                lines.push(line);
            }

            // Move to the next token if this is whitespace
            if token.token_type == TokenType::Whitespace {
                // Load LHS from next token in loop
                if let Some(next_token) = tokens_iter.peek() {
                    start = next_token.lhs;
                }
            } else {
                start = token.lhs;
            }
        }

        line.append(&mut token.glyphs);
    }

    // Handle the remainders
    if !line.is_empty() {
        let line = std::mem::take(&mut line);
        //debug!(target: "text::apply_wrap", "adding rem line: {}", glyph_str(&line));
        lines.push(line);
    }

    lines
}

/// Splits any Word token that exceeds the line width.
/// So Word("aaaaaaaaaaaaaaa") => [Word("aaaaaaaa"), Word("aaaaaaa")].
pub fn restrict_word_len(
    font_size: f32,
    window_scale: f32,
    raw_tokens: Vec<Token>,
    line_width: f32,
) -> Vec<Token> {
    let mut tokens = vec![];
    for token in raw_tokens {
        match token.token_type {
            TokenType::Word => {
                assert!(!token.glyphs.is_empty());
                let token_width = token.rhs - token.lhs;
                // No change required. This is the usual code path
                if token_width < line_width {
                    tokens.push(token);
                    continue
                }
            }
            _ => {
                tokens.push(token);
                continue
            }
        }

        // OK we have encountered a Word that is very long. Lets split it up
        // into multiple Words each under line_width.

        let glyphs2 = token.glyphs.clone();
        let glyph_pos_iter = GlyphPositionIter::new(font_size, window_scale, &glyphs2, 0.);
        let mut token_glyphs = vec![];
        let mut lhs = -1.;
        let mut rhs = 0.;

        // Just loop through each glyph. When the running total exceeds line_width
        // then push the buffer, and start again.
        // Very basic stuff.
        for (pos, glyph) in glyph_pos_iter.zip(token.glyphs.into_iter()) {
            if lhs < 0. {
                lhs = pos.x;
            }
            rhs = pos.x + pos.w;

            token_glyphs.push(glyph);

            let curr_width = rhs - lhs;
            // Line width exceeded. Do our thing.
            if curr_width > line_width {
                let token = Token {
                    token_type: TokenType::Word,
                    lhs,
                    rhs,
                    glyphs: std::mem::take(&mut token_glyphs),
                };
                tokens.push(token);
                lhs = -1.;
            }
        }

        // Take care of any remainders left over.
        if !token_glyphs.is_empty() {
            let token = Token {
                token_type: TokenType::Word,
                lhs,
                rhs,
                glyphs: std::mem::take(&mut token_glyphs),
            };
            tokens.push(token);
        }
    }
    tokens
}

pub fn wrap(
    line_width: f32,
    font_size: f32,
    window_scale: f32,
    glyphs: &Vec<Glyph>,
) -> Vec<Vec<Glyph>> {
    let tokens = tokenize(font_size, window_scale, glyphs);

    //debug!(target: "text::wrap", "tokenized words {:?}",
    //       words.iter().map(|w| w.as_str()).collect::<Vec<_>>());

    let tokens = restrict_word_len(font_size, window_scale, tokens, line_width);

    let lines = apply_wrap(line_width, tokens);

    //if lines.len() > 1 {
    //    debug!(target: "text::wrap", "wrapped line: {}", glyph_str(glyphs));
    //    for line in &lines {
    //        debug!(target: "text::wrap", "-> {}", glyph_str(line));
    //    }
    //}

    lines
}

#[cfg(test)]
mod tests {
    use super::{super::*, *};

    #[test]
    fn wrap_simple() {
        let shaper = TextShaper::new();
        let glyphs = shaper.shape("hello world 123".to_string(), 32., 1.);

        let wrapped = wrap(200., 32., 1., &glyphs);
        assert_eq!(wrapped.len(), 3);
        assert_eq!(glyph_str(&wrapped[0]), "hello ");
        assert_eq!(glyph_str(&wrapped[1]), "world ");
        assert_eq!(glyph_str(&wrapped[2]), "123");
    }

    #[test]
    fn wrap_long() {
        let shaper = TextShaper::new();
        let glyphs = shaper.shape("aaaaaaaaaaaaaaa".to_string(), 32., 1.);

        let wrapped = wrap(200., 32., 1., &glyphs);
        assert_eq!(wrapped.len(), 2);
        assert_eq!(glyph_str(&wrapped[0]), "aaaaaaaa");
        assert_eq!(glyph_str(&wrapped[1]), "aaaaaaa");
    }
}
