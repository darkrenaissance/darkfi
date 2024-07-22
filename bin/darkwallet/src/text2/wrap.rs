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
    fn as_str(&self) -> String {
        glyph_str(&self.glyphs)
    }
}

/// Get the string represented by a vec of glyphs. Useful for debugging.
fn glyph_str(glyphs: &Vec<Glyph>) -> String {
    glyphs.iter().map(|g| g.substr.as_str()).collect::<Vec<_>>().join("")
}

fn tokenize(font_size: f32, glyphs: &Vec<Glyph>) -> Vec<Token> {
    let glyph_pos_iter = GlyphPositionIter::new(font_size, glyphs, 0.);

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
            rhs = 0.;
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
/// Whitespace is perserved unless the word wraps.
fn apply_wrap(line_width: f32, tokens: Vec<Token>) -> Vec<Vec<Glyph>> {
    let mut lines = vec![];
    let mut line = vec![];
    let mut start = 0.;

    for (i, mut token) in tokens.into_iter().enumerate() {
        assert!(token.token_type != TokenType::Null);

        // Triggered by if below
        if start < 0. {
            assert_eq!(token.token_type, TokenType::Word);
            start = token.lhs;
        }

        // Does this token cross over the end of the line?
        if token.rhs > start + line_width {
            // Start a new line
            let line = std::mem::take(&mut line);
            //debug!(target: "text::apply_wrap", "adding line: {}", glyph_str(&line));
            lines.push(line);

            // Whitespace tokens that cause wrapping are just discarded.
            if token.token_type == TokenType::Whitespace {
                // Load LHS from next token in loop
                start = -1.;
                continue
            }

            assert_eq!(token.token_type, TokenType::Word);
            start = token.lhs;
        }

        line.append(&mut token.glyphs);
    }

    // Handle the remainders
    if !line.is_empty() {
        let line = std::mem::take(&mut line);
        //debug!(target: "text::apply_wrap", "adding line: {}", glyph_str(&line));
        lines.push(line);
    }

    lines
}

pub fn wrap(line_width: f32, font_size: f32, glyphs: &Vec<Glyph>) -> Vec<Vec<Glyph>> {
    let tokens = tokenize(font_size, glyphs);

    //debug!(target: "text::wrap", "tokenized words {:?}",
    //       words.iter().map(|w| w.as_str()).collect::<Vec<_>>());

    let lines = apply_wrap(line_width, tokens);

    //if lines.len() > 1 {
    //    debug!(target: "text::wrap", "wrapped line: {}", glyph_str(glyphs));
    //    for line in &lines {
    //        debug!(target: "text::wrap", "-> {}", glyph_str(line));
    //    }
    //}

    lines
}
