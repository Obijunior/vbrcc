/// Replace each comment and each backslash-newline pair with spaces.
///
/// The result has the same number of characters as the input, so an offset into the
/// result is also an offset into the original file. This is why a span from a header
/// needs no offset arithmetic.
///
/// A newline inside a block comment stays a newline, so the line structure does not
/// change and the caller still finds every directive.
pub fn normalize(text: &str) -> Vec<char> {
    let src: Vec<char> = text.chars().collect();
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;

    while i < src.len() {
        let c = src[i];

        // Backslash-newline: blank the pair (or triple, for CRLF) so the two
        // physical lines become one logical line.
        if c == '\\' && i + 1 < src.len() {
            if src[i + 1] == '\n' {
                out.push(' ');
                out.push(' ');
                i += 2;
                continue;
            }
            if src[i + 1] == '\r' && i + 2 < src.len() && src[i + 2] == '\n' {
                out.push(' ');
                out.push(' ');
                out.push(' ');
                i += 3;
                continue;
            }
        }

        // String and char literals are opaque: copy them through verbatim so a
        // `/*` inside one is not mistaken for a comment.
        if c == '"' || c == '\'' {
            let quote = c;
            out.push(c);
            i += 1;
            while i < src.len() {
                let d = src[i];
                out.push(d);
                i += 1;
                if d == '\\' && i < src.len() {
                    out.push(src[i]);
                    i += 1;
                    continue;
                }
                if d == quote {
                    break;
                }
            }
            continue;
        }

        if c == '/' && i + 1 < src.len() && src[i + 1] == '/' {
            while i < src.len() && src[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }

        if c == '/' && i + 1 < src.len() && src[i + 1] == '*' {
            out.push(' ');
            out.push(' ');
            i += 2;
            while i < src.len() {
                if src[i] == '*' && i + 1 < src.len() && src[i + 1] == '/' {
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                    break;
                }
                // Interior newlines survive so line structure is unchanged.
                out.push(if src[i] == '\n' { '\n' } else { ' ' });
                i += 1;
            }
            continue;
        }

        out.push(c);
        i += 1;
    }

    debug_assert_eq!(out.len(), src.len(), "normalize must preserve length");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(s: &str) -> String {
        normalize(s).into_iter().collect()
    }

    #[test]
    fn length_is_always_preserved() {
        for src in [
            "int x;",
            "a /* comment */ b",
            "a // comment\nb",
            "a \\\n b",
            "/* multi\nline */ x",
            "\"/* not a comment */\"",
            "",
        ] {
            assert_eq!(
                normalize(src).len(),
                src.chars().count(),
                "length changed for {src:?}"
            );
        }
    }

    #[test]
    fn line_comment_becomes_spaces_but_keeps_the_newline() {
        // "a // hi" is 7 chars; only the 5 comment chars (`// hi`) are blanked,
        // and the space at index 1 was already a space. So: 'a' + 6 spaces.
        assert_eq!(norm("a // hi\nb"), "a      \nb");
    }

    #[test]
    fn block_comment_becomes_spaces() {
        assert_eq!(norm("a /* hi */ b"), "a          b");
    }

    #[test]
    fn block_comment_keeps_interior_newlines() {
        // Line structure must survive, or a directive on the next line is missed.
        assert_eq!(norm("a /*\nx\n*/ b"), "a   \n \n   b");
    }

    #[test]
    fn directive_inside_a_block_comment_is_not_a_directive() {
        let out = norm("/*\n#define X 1\n*/\nint y;");
        let second_line = out.lines().nth(1).unwrap();
        assert!(second_line.trim().is_empty(), "got {second_line:?}");
    }

    #[test]
    fn backslash_newline_joins_lines() {
        // The newline is blanked, so the two physical lines are now one.
        let out = norm("#define A 1 \\\n    2");
        assert_eq!(out.lines().count(), 1, "got {out:?}");
        assert!(out.starts_with("#define A 1"));
    }

    #[test]
    fn backslash_crlf_joins_lines() {
        let out = norm("#define A 1 \\\r\n    2");
        assert_eq!(out.lines().count(), 1, "got {out:?}");
    }

    #[test]
    fn comment_markers_inside_string_literals_are_left_alone() {
        assert_eq!(norm("char *s = \"/* hi */\";"), "char *s = \"/* hi */\";");
    }

    #[test]
    fn comment_markers_inside_char_literals_are_left_alone() {
        assert_eq!(norm("char c = '/';"), "char c = '/';");
    }

    #[test]
    fn escaped_quote_does_not_end_the_string() {
        assert_eq!(norm("\"a\\\"/*\" b"), "\"a\\\"/*\" b");
    }

    #[test]
    fn unterminated_block_comment_runs_to_end_of_file() {
        // The lexer reports this error later; normalize just blanks what it sees.
        assert_eq!(norm("x /* y"), "x     ");
    }
}