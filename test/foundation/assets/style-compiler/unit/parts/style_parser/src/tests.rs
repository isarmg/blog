/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[cfg(any())]
extern crate test;

#[cfg(any())]
use self::test::Bencher;

use super::{
    BasicParseError, BasicParseErrorKind, CowRcStr, Delimiter, ParseError, ParseErrorKind, Parser,
    ParserInput, SourceLocation, ToCss, Token,
};

#[test]
fn expect_no_error_token() {
    let mut input = ParserInput::new("foo 4px ( / { !bar }");
    assert!(Parser::new(&mut input).expect_no_error_token().is_ok());
    let mut input = ParserInput::new(")");
    assert!(Parser::new(&mut input).expect_no_error_token().is_err());
    let mut input = ParserInput::new("}");
    assert!(Parser::new(&mut input).expect_no_error_token().is_err());
    let mut input = ParserInput::new("(a){]");
    assert!(Parser::new(&mut input).expect_no_error_token().is_err());
    let mut input = ParserInput::new("'\n'");
    assert!(Parser::new(&mut input).expect_no_error_token().is_err());
    let mut input = ParserInput::new("url('\n'");
    assert!(Parser::new(&mut input).expect_no_error_token().is_err());
    let mut input = ParserInput::new("url(a b)");
    assert!(Parser::new(&mut input).expect_no_error_token().is_err());
    let mut input = ParserInput::new("url(\u{7F}))");
    assert!(Parser::new(&mut input).expect_no_error_token().is_err());
}

/// https://github.com/servo/rust-cssparser/issues/71
#[test]
fn outer_block_end_consumed() {
    let mut input = ParserInput::new("(calc(true))");
    let mut input = Parser::new(&mut input);
    assert!(input.expect_parenthesis_block().is_ok());
    assert!(input
        .parse_nested_block(|input| input
            .expect_function_matching("calc")
            .map_err(Into::<ParseError<()>>::into))
        .is_ok());
    println!("{:?}", input.position());
    assert!(input.next().is_err());
}

/// https://github.com/servo/rust-cssparser/issues/174
#[test]
fn bad_url_slice_out_of_bounds() {
    let mut input = ParserInput::new("url(\u{1}\\");
    let mut parser = Parser::new(&mut input);
    let result = parser.next_including_whitespace_and_comments(); // This used to panic
    assert_eq!(result, Ok(&Token::BadUrl("\u{1}\\".into())));
}

/// https://bugzilla.mozilla.org/show_bug.cgi?id=1383975
#[test]
fn bad_url_slice_not_at_char_boundary() {
    let mut input = ParserInput::new("url(9\n۰");
    let mut parser = Parser::new(&mut input);
    let result = parser.next_including_whitespace_and_comments(); // This used to panic
    assert_eq!(result, Ok(&Token::BadUrl("9\n۰".into())));
}

#[test]
fn unquoted_url_escaping() {
    let token = Token::UnquotedUrl(
        "\
         \x01\x02\x03\x04\x05\x06\x07\x08\t\n\x0b\x0c\r\x0e\x0f\x10\
         \x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f \
         !\"#$%&\'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]\
         ^_`abcdefghijklmnopqrstuvwxyz{|}~\x7fé\
         "
        .into(),
    );
    let serialized = token.to_css_string();
    assert_eq!(
        serialized,
        "\
         url(\
         \\1 \\2 \\3 \\4 \\5 \\6 \\7 \\8 \\9 \\a \\b \\c \\d \\e \\f \\10 \
         \\11 \\12 \\13 \\14 \\15 \\16 \\17 \\18 \\19 \\1a \\1b \\1c \\1d \\1e \\1f \\20 \
         !\\\"#$%&\\'\\(\\)*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\\\]\
         ^_`abcdefghijklmnopqrstuvwxyz{|}~\\7f é\
         )\
         "
    );
    let mut input = ParserInput::new(&serialized);
    assert_eq!(Parser::new(&mut input).next(), Ok(&token));
}

#[test]
fn test_expect_url() {
    fn parse<'a>(s: &mut ParserInput<'a>) -> Result<CowRcStr<'a>, BasicParseError<'a>> {
        Parser::new(s).expect_url()
    }
    let mut input = ParserInput::new("url()");
    assert_eq!(parse(&mut input).unwrap(), "");
    let mut input = ParserInput::new("url( ");
    assert_eq!(parse(&mut input).unwrap(), "");
    let mut input = ParserInput::new("url( abc");
    assert_eq!(parse(&mut input).unwrap(), "abc");
    let mut input = ParserInput::new("url( abc \t)");
    assert_eq!(parse(&mut input).unwrap(), "abc");
    let mut input = ParserInput::new("url( 'abc' \t)");
    assert_eq!(parse(&mut input).unwrap(), "abc");
    let mut input = ParserInput::new("url(abc more stuff)");
    assert!(parse(&mut input).is_err());
    // The grammar at https://drafts.csswg.org/css-values/#urls plans for `<url-modifier>*`
    // at the position of "more stuff", but no such modifier is defined yet.
    let mut input = ParserInput::new("url('abc' more stuff)");
    assert!(parse(&mut input).is_err());
}

#[test]
fn parse_comma_separated_ignoring_errors() {
    let input = "red, green something, yellow, whatever, blue";
    let mut input = ParserInput::new(input);
    let mut input = Parser::new(&mut input);
    let result = input.parse_comma_separated_ignoring_errors(|input| {
        let loc = input.current_source_location();
        let ident = input.expect_ident()?;
        crate::style_parser::color::parse_named_color(ident).map_err(|()| {
            loc.new_unexpected_token_error::<ParseError<()>>(Token::Ident(ident.clone()))
        })
    });
    assert_eq!(result.len(), 3);
    assert_eq!(result[0], (255, 0, 0));
    assert_eq!(result[1], (255, 255, 0));
    assert_eq!(result[2], (0, 0, 255));
}

#[test]
fn serialize_bad_tokens() {
    let mut input = ParserInput::new("url(foo\\) b\\)ar)'ba\\'\"z\n4");
    let mut parser = Parser::new(&mut input);

    let token = parser.next().unwrap().clone();
    assert!(matches!(token, Token::BadUrl(_)));
    assert_eq!(token.to_css_string(), "url(foo\\) b\\)ar)");

    let token = parser.next().unwrap().clone();
    assert!(matches!(token, Token::BadString(_)));
    assert_eq!(token.to_css_string(), "\"ba'\\\"z");

    let token = parser.next().unwrap().clone();
    assert!(matches!(token, Token::Number { .. }));
    assert_eq!(token.to_css_string(), "4");

    assert!(parser.next().is_err());
}

#[test]
fn line_numbers() {
    let mut input = ParserInput::new(concat!(
        "fo\\30\r\n",
        "0o bar/*\n",
        "*/baz\r\n",
        "\n",
        "url(\r\n",
        "  u \r\n",
        ")\"a\\\r\n",
        "b\""
    ));
    let mut input = Parser::new(&mut input);
    assert_eq!(
        input.current_source_location(),
        SourceLocation { line: 0, column: 1 }
    );
    assert_eq!(
        input.next_including_whitespace(),
        Ok(&Token::Ident("fo00o".into()))
    );
    assert_eq!(
        input.current_source_location(),
        SourceLocation { line: 1, column: 3 }
    );
    assert_eq!(
        input.next_including_whitespace(),
        Ok(&Token::WhiteSpace(" "))
    );
    assert_eq!(
        input.current_source_location(),
        SourceLocation { line: 1, column: 4 }
    );
    assert_eq!(
        input.next_including_whitespace(),
        Ok(&Token::Ident("bar".into()))
    );
    assert_eq!(
        input.current_source_location(),
        SourceLocation { line: 1, column: 7 }
    );
    assert_eq!(
        input.next_including_whitespace_and_comments(),
        Ok(&Token::Comment("\n"))
    );
    assert_eq!(
        input.current_source_location(),
        SourceLocation { line: 2, column: 3 }
    );
    assert_eq!(
        input.next_including_whitespace(),
        Ok(&Token::Ident("baz".into()))
    );
    assert_eq!(
        input.current_source_location(),
        SourceLocation { line: 2, column: 6 }
    );
    let state = input.state();

    assert_eq!(
        input.next_including_whitespace(),
        Ok(&Token::WhiteSpace("\r\n\n"))
    );
    assert_eq!(
        input.current_source_location(),
        SourceLocation { line: 4, column: 1 }
    );

    assert_eq!(
        state.source_location(),
        SourceLocation { line: 2, column: 6 }
    );

    assert_eq!(
        input.next_including_whitespace(),
        Ok(&Token::UnquotedUrl("u".into()))
    );
    assert_eq!(
        input.current_source_location(),
        SourceLocation { line: 6, column: 2 }
    );

    assert_eq!(
        input.next_including_whitespace(),
        Ok(&Token::QuotedString("ab".into()))
    );
    assert_eq!(
        input.current_source_location(),
        SourceLocation { line: 7, column: 3 }
    );
    assert!(input.next_including_whitespace().is_err());
}

#[test]
fn overflow() {
    use std::iter::repeat;

    let css = r"
         2147483646
         2147483647
         2147483648
         10000000000000
         1000000000000000000000000000000000000000
         1{309 zeros}

         -2147483647
         -2147483648
         -2147483649
         -10000000000000
         -1000000000000000000000000000000000000000
         -1{309 zeros}

         3.30282347e+38
         3.40282347e+38
         3.402824e+38

         -3.30282347e+38
         -3.40282347e+38
         -3.402824e+38

    "
    .replace("{309 zeros}", &repeat('0').take(309).collect::<String>());
    let mut input = ParserInput::new(&css);
    let mut input = Parser::new(&mut input);

    assert_eq!(input.expect_integer(), Ok(2147483646));
    assert_eq!(input.expect_integer(), Ok(2147483647));
    assert_eq!(input.expect_integer(), Ok(2147483647)); // Clamp on overflow
    assert_eq!(input.expect_integer(), Ok(2147483647));
    assert_eq!(input.expect_integer(), Ok(2147483647));
    assert_eq!(input.expect_integer(), Ok(2147483647));

    assert_eq!(input.expect_integer(), Ok(-2147483647));
    assert_eq!(input.expect_integer(), Ok(-2147483648));
    assert_eq!(input.expect_integer(), Ok(-2147483648)); // Clamp on overflow
    assert_eq!(input.expect_integer(), Ok(-2147483648));
    assert_eq!(input.expect_integer(), Ok(-2147483648));
    assert_eq!(input.expect_integer(), Ok(-2147483648));

    assert_eq!(input.expect_number(), Ok(3.30282347e+38));
    assert_eq!(input.expect_number(), Ok(f32::MAX));
    assert_eq!(input.expect_number(), Ok(f32::INFINITY));
    assert!(f32::MAX != f32::INFINITY);

    assert_eq!(input.expect_number(), Ok(-3.30282347e+38));
    assert_eq!(input.expect_number(), Ok(f32::MIN));
    assert_eq!(input.expect_number(), Ok(f32::NEG_INFINITY));
    assert!(f32::MIN != f32::NEG_INFINITY);
}

#[test]
fn line_delimited() {
    let mut input = ParserInput::new(" { foo ; bar } baz;,");
    let mut input = Parser::new(&mut input);
    assert_eq!(input.next(), Ok(&Token::CurlyBracketBlock));
    assert!({
        let result: Result<_, ParseError<()>> =
            input.parse_until_after(Delimiter::Semicolon, |_| Ok(42));
        result
    }
    .is_err());
    assert_eq!(input.next(), Ok(&Token::Comma));
    assert!(input.next().is_err());
}

#[test]
fn identifier_serialization() {
    // Null bytes
    assert_eq!(Token::Ident("\0".into()).to_css_string(), "\u{FFFD}");
    assert_eq!(Token::Ident("a\0".into()).to_css_string(), "a\u{FFFD}");
    assert_eq!(Token::Ident("\0b".into()).to_css_string(), "\u{FFFD}b");
    assert_eq!(Token::Ident("a\0b".into()).to_css_string(), "a\u{FFFD}b");

    // Replacement character
    assert_eq!(Token::Ident("\u{FFFD}".into()).to_css_string(), "\u{FFFD}");
    assert_eq!(
        Token::Ident("a\u{FFFD}".into()).to_css_string(),
        "a\u{FFFD}"
    );
    assert_eq!(
        Token::Ident("\u{FFFD}b".into()).to_css_string(),
        "\u{FFFD}b"
    );
    assert_eq!(
        Token::Ident("a\u{FFFD}b".into()).to_css_string(),
        "a\u{FFFD}b"
    );

    // Number prefix
    assert_eq!(Token::Ident("0a".into()).to_css_string(), "\\30 a");
    assert_eq!(Token::Ident("1a".into()).to_css_string(), "\\31 a");
    assert_eq!(Token::Ident("2a".into()).to_css_string(), "\\32 a");
    assert_eq!(Token::Ident("3a".into()).to_css_string(), "\\33 a");
    assert_eq!(Token::Ident("4a".into()).to_css_string(), "\\34 a");
    assert_eq!(Token::Ident("5a".into()).to_css_string(), "\\35 a");
    assert_eq!(Token::Ident("6a".into()).to_css_string(), "\\36 a");
    assert_eq!(Token::Ident("7a".into()).to_css_string(), "\\37 a");
    assert_eq!(Token::Ident("8a".into()).to_css_string(), "\\38 a");
    assert_eq!(Token::Ident("9a".into()).to_css_string(), "\\39 a");

    // Letter number prefix
    assert_eq!(Token::Ident("a0b".into()).to_css_string(), "a0b");
    assert_eq!(Token::Ident("a1b".into()).to_css_string(), "a1b");
    assert_eq!(Token::Ident("a2b".into()).to_css_string(), "a2b");
    assert_eq!(Token::Ident("a3b".into()).to_css_string(), "a3b");
    assert_eq!(Token::Ident("a4b".into()).to_css_string(), "a4b");
    assert_eq!(Token::Ident("a5b".into()).to_css_string(), "a5b");
    assert_eq!(Token::Ident("a6b".into()).to_css_string(), "a6b");
    assert_eq!(Token::Ident("a7b".into()).to_css_string(), "a7b");
    assert_eq!(Token::Ident("a8b".into()).to_css_string(), "a8b");
    assert_eq!(Token::Ident("a9b".into()).to_css_string(), "a9b");

    // Dash number prefix
    assert_eq!(Token::Ident("-0a".into()).to_css_string(), "-\\30 a");
    assert_eq!(Token::Ident("-1a".into()).to_css_string(), "-\\31 a");
    assert_eq!(Token::Ident("-2a".into()).to_css_string(), "-\\32 a");
    assert_eq!(Token::Ident("-3a".into()).to_css_string(), "-\\33 a");
    assert_eq!(Token::Ident("-4a".into()).to_css_string(), "-\\34 a");
    assert_eq!(Token::Ident("-5a".into()).to_css_string(), "-\\35 a");
    assert_eq!(Token::Ident("-6a".into()).to_css_string(), "-\\36 a");
    assert_eq!(Token::Ident("-7a".into()).to_css_string(), "-\\37 a");
    assert_eq!(Token::Ident("-8a".into()).to_css_string(), "-\\38 a");
    assert_eq!(Token::Ident("-9a".into()).to_css_string(), "-\\39 a");

    // Double dash prefix
    assert_eq!(Token::Ident("--a".into()).to_css_string(), "--a");

    // Various tests
    assert_eq!(
        Token::Ident("\x01\x02\x1E\x1F".into()).to_css_string(),
        "\\1 \\2 \\1e \\1f "
    );
    assert_eq!(
        Token::Ident("\u{0080}\x2D\x5F\u{00A9}".into()).to_css_string(),
        "\u{0080}\x2D\x5F\u{00A9}"
    );
    assert_eq!(Token::Ident("\x7F\u{0080}\u{0081}\u{0082}\u{0083}\u{0084}\u{0085}\u{0086}\u{0087}\u{0088}\u{0089}\
        \u{008A}\u{008B}\u{008C}\u{008D}\u{008E}\u{008F}\u{0090}\u{0091}\u{0092}\u{0093}\u{0094}\u{0095}\u{0096}\
        \u{0097}\u{0098}\u{0099}\u{009A}\u{009B}\u{009C}\u{009D}\u{009E}\u{009F}".into()).to_css_string(),
        "\\7f \u{0080}\u{0081}\u{0082}\u{0083}\u{0084}\u{0085}\u{0086}\u{0087}\u{0088}\u{0089}\u{008A}\u{008B}\u{008C}\
        \u{008D}\u{008E}\u{008F}\u{0090}\u{0091}\u{0092}\u{0093}\u{0094}\u{0095}\u{0096}\u{0097}\u{0098}\u{0099}\
        \u{009A}\u{009B}\u{009C}\u{009D}\u{009E}\u{009F}");
    assert_eq!(
        Token::Ident("\u{00A0}\u{00A1}\u{00A2}".into()).to_css_string(),
        "\u{00A0}\u{00A1}\u{00A2}"
    );
    assert_eq!(
        Token::Ident("a0123456789b".into()).to_css_string(),
        "a0123456789b"
    );
    assert_eq!(
        Token::Ident("abcdefghijklmnopqrstuvwxyz".into()).to_css_string(),
        "abcdefghijklmnopqrstuvwxyz"
    );
    assert_eq!(
        Token::Ident("ABCDEFGHIJKLMNOPQRSTUVWXYZ".into()).to_css_string(),
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    );
    assert_eq!(
        Token::Ident("\x20\x21\x78\x79".into()).to_css_string(),
        "\\ \\!xy"
    );

    // astral symbol (U+1D306 TETRAGRAM FOR CENTRE)
    assert_eq!(
        Token::Ident("\u{1D306}".into()).to_css_string(),
        "\u{1D306}"
    );
}

#[bench]
#[cfg(any())]
fn delimiter_from_byte(b: &mut Bencher) {
    use crate::style_parser::Delimiters;
    b.iter(|| {
        for _ in 0..1000 {
            for i in 0..256 {
                std::hint::black_box(Delimiters::from_byte(Some(i as u8)));
            }
        }
    })
}

#[cfg(any())]
const BACKGROUND_IMAGE: &'static str = include_str!("big-data-url.css");

#[cfg(any())]
#[bench]
fn unquoted_url(b: &mut Bencher) {
    b.iter(|| {
        let mut input = ParserInput::new(BACKGROUND_IMAGE);
        let mut input = Parser::new(&mut input);
        input.look_for_var_or_env_functions();

        let result = input.try_parse(|input| input.expect_url());

        assert!(result.is_ok());

        input.seen_var_or_env_functions();
        (result.is_ok(), input.seen_var_or_env_functions())
    })
}

#[cfg_attr(all(miri, any()), ignore)]
#[cfg(any())]
#[bench]
fn numeric(b: &mut Bencher) {
    b.iter(|| {
        for _ in 0..1000000 {
            let mut input = ParserInput::new("10px");
            let mut input = Parser::new(&mut input);
            let _ = test::black_box(input.next());
        }
    })
}

#[cfg_attr(all(miri, any()), ignore)]
#[test]
fn no_stack_overflow_multiple_nested_blocks() {
    let mut input: String = "{{".into();
    for _ in 0..20 {
        let dup = input.clone();
        input.push_str(&dup);
    }
    let mut input = ParserInput::new(&input);
    let mut input = Parser::new(&mut input);
    while let Ok(..) = input.next() {}
}

/// A previous version of procedural-masquerade had a bug where it
/// would normalize consecutive whitespace to a single space,
/// including in string literals.
#[test]
fn procedural_masquerade_whitespace() {
    ascii_case_insensitive_phf_map! {
        map -> () = {
            "  \t\n" => ()
        }
    }
    assert_eq!(map::get("  \t\n"), Some(&()));
    assert_eq!(map::get(" "), None);

    match_ignore_ascii_case! { "  \t\n",
        " " => panic!("1"),
        "  \t\n" => {},
        _ => panic!("2"),
    }

    match_ignore_ascii_case! { " ",
        "  \t\n" => panic!("3"),
        " " => {},
        _ => panic!("4"),
    }
}

#[test]
fn parse_until_before_stops_at_delimiter_or_end_of_input() {
    // For all j and k, inputs[i].1[j] should parse the same as inputs[i].1[k]
    // when we use delimiters inputs[i].0.
    let inputs = vec![
        (
            Delimiter::Bang | Delimiter::Semicolon,
            // Note that the ';extra' is fine, because the ';' acts the same as
            // the end of input.
            vec!["token stream;extra", "token stream!", "token stream"],
        ),
        (Delimiter::Bang | Delimiter::Semicolon, vec![";", "!", ""]),
    ];
    for equivalent in inputs {
        for (j, x) in equivalent.1.iter().enumerate() {
            for y in equivalent.1[j + 1..].iter() {
                let mut ix = ParserInput::new(x);
                let mut ix = Parser::new(&mut ix);

                let mut iy = ParserInput::new(y);
                let mut iy = Parser::new(&mut iy);

                let _ = ix.parse_until_before::<_, _, ()>(equivalent.0, |ix| {
                    iy.parse_until_before::<_, _, ()>(equivalent.0, |iy| {
                        loop {
                            let ox = ix.next();
                            let oy = iy.next();
                            assert_eq!(ox, oy);
                            if let Err(_) = ox {
                                break;
                            }
                        }
                        Ok(())
                    })
                });
            }
        }
    }
}

#[test]
fn parser_maintains_current_line() {
    let mut input = ParserInput::new("ident ident;\nident ident ident;\nident");
    let mut parser = Parser::new(&mut input);
    assert_eq!(parser.current_line(), "ident ident;");
    assert_eq!(parser.next(), Ok(&Token::Ident("ident".into())));
    assert_eq!(parser.next(), Ok(&Token::Ident("ident".into())));
    assert_eq!(parser.next(), Ok(&Token::Semicolon));

    assert_eq!(parser.next(), Ok(&Token::Ident("ident".into())));
    assert_eq!(parser.current_line(), "ident ident ident;");
    assert_eq!(parser.next(), Ok(&Token::Ident("ident".into())));
    assert_eq!(parser.next(), Ok(&Token::Ident("ident".into())));
    assert_eq!(parser.next(), Ok(&Token::Semicolon));

    assert_eq!(parser.next(), Ok(&Token::Ident("ident".into())));
    assert_eq!(parser.current_line(), "ident");
}

#[test]
fn cdc_regression_test() {
    let mut input = ParserInput::new("-->x");
    let mut parser = Parser::new(&mut input);
    parser.skip_cdc_and_cdo();
    assert_eq!(parser.next(), Ok(&Token::Ident("x".into())));
    assert_eq!(
        parser.next(),
        Err(BasicParseError {
            kind: BasicParseErrorKind::EndOfInput,
            location: SourceLocation { line: 0, column: 5 }
        })
    );
}

#[test]
fn parse_entirely_reports_first_error() {
    #[derive(PartialEq, Debug)]
    enum E {
        Foo,
    }
    let mut input = ParserInput::new("ident");
    let mut parser = Parser::new(&mut input);
    let result: Result<(), _> = parser.parse_entirely(|p| Err(p.new_custom_error(E::Foo)));
    assert_eq!(
        result,
        Err(ParseError {
            kind: ParseErrorKind::Custom(E::Foo),
            location: SourceLocation { line: 0, column: 1 },
        })
    );
}

#[test]
fn parse_sourcemapping_comments() {
    let tests = vec![
        ("/*# sourceMappingURL=here*/", Some("here")),
        ("/*# sourceMappingURL=here  */", Some("here")),
        ("/*@ sourceMappingURL=here*/", Some("here")),
        (
            "/*@ sourceMappingURL=there*/ /*# sourceMappingURL=here*/",
            Some("here"),
        ),
        ("/*# sourceMappingURL=here there  */", Some("here")),
        ("/*# sourceMappingURL=  here  */", Some("")),
        ("/*# sourceMappingURL=*/", Some("")),
        ("/*# sourceMappingUR=here  */", None),
        ("/*! sourceMappingURL=here  */", None),
        ("/*# sourceMappingURL = here  */", None),
        ("/*   # sourceMappingURL=here   */", None),
    ];

    for test in tests {
        let mut input = ParserInput::new(test.0);
        let mut parser = Parser::new(&mut input);
        while let Ok(_) = parser.next_including_whitespace() {}
        assert_eq!(parser.current_source_map_url(), test.1);
    }
}

#[test]
fn parse_sourceurl_comments() {
    let tests = vec![
        ("/*# sourceURL=here*/", Some("here")),
        ("/*# sourceURL=here  */", Some("here")),
        ("/*@ sourceURL=here*/", Some("here")),
        ("/*@ sourceURL=there*/ /*# sourceURL=here*/", Some("here")),
        ("/*# sourceURL=here there  */", Some("here")),
        ("/*# sourceURL=  here  */", Some("")),
        ("/*# sourceURL=*/", Some("")),
        ("/*# sourceMappingUR=here  */", None),
        ("/*! sourceURL=here  */", None),
        ("/*# sourceURL = here  */", None),
        ("/*   # sourceURL=here   */", None),
    ];

    for test in tests {
        let mut input = ParserInput::new(test.0);
        let mut parser = Parser::new(&mut input);
        while let Ok(_) = parser.next_including_whitespace() {}
        assert_eq!(parser.current_source_url(), test.1);
    }
}

#[cfg_attr(all(miri, any()), ignore)]
#[test]
fn roundtrip_percentage_token() {
    fn test_roundtrip(value: &str) {
        let mut input = ParserInput::new(value);
        let mut parser = Parser::new(&mut input);
        let token = parser.next().unwrap();
        assert_eq!(token.to_css_string(), value);
    }
    // Test simple number serialization
    for i in 0..101 {
        test_roundtrip(&format!("{}%", i));
        for j in 0..10 {
            if j != 0 {
                test_roundtrip(&format!("{}.{}%", i, j));
            }
            for k in 1..10 {
                test_roundtrip(&format!("{}.{}{}%", i, j, k));
            }
        }
    }
}

#[test]
fn utf16_columns() {
    // This particular test serves two purposes.  First, it checks
    // that the column number computations are correct.  Second, it
    // checks that tokenizer code paths correctly differentiate
    // between the different UTF-8 encoding bytes.  In particular
    // different leader bytes and continuation bytes are treated
    // differently, so we make sure to include all lengths in the
    // tests, using the string "QΡ✈🆒".  Also, remember that because
    // the column is in units of UTF-16, the 4-byte sequence results
    // in two columns.
    let tests = vec![
        ("", 1),
        ("ascii", 6),
        ("/*QΡ✈🆒*/", 10),
        ("'QΡ✈🆒*'", 9),
        ("\"\\\"'QΡ✈🆒*'", 12),
        ("\\Q\\Ρ\\✈\\🆒", 10),
        ("QΡ✈🆒", 6),
        ("QΡ✈🆒\\Q\\Ρ\\✈\\🆒", 15),
        ("newline\r\nQΡ✈🆒", 6),
        ("url(QΡ✈🆒\\Q\\Ρ\\✈\\🆒)", 20),
        ("url(QΡ✈🆒)", 11),
        ("url(\r\nQΡ✈🆒\\Q\\Ρ\\✈\\🆒)", 16),
        ("url(\r\nQΡ✈🆒\\Q\\Ρ\\✈\\🆒", 15),
        ("url(\r\nQΡ✈🆒\\Q\\Ρ\\✈\\🆒 x", 17),
        ("QΡ✈🆒()", 8),
        // Test that under/over-flow of current_line_start_position is
        // handled properly; see the special case in consume_4byte_intro.
        ("🆒", 3),
    ];

    for test in tests {
        let mut input = ParserInput::new(test.0);
        let mut parser = Parser::new(&mut input);

        // Read all tokens.
        loop {
            match parser.next() {
                Err(BasicParseError {
                    kind: BasicParseErrorKind::EndOfInput,
                    ..
                }) => {
                    break;
                }
                Err(_) => {
                    assert!(false);
                }
                Ok(_) => {}
            };
        }

        // Check the resulting column.
        assert_eq!(parser.current_source_location().column, test.1);
    }
}

#[test]
fn servo_define_css_keyword_enum() {
    macro_rules! define_css_keyword_enum {
        (pub enum $name:ident { $($variant:ident = $css:pat,)+ }) => {
            #[derive(PartialEq, Debug)]
            pub enum $name {
                $($variant),+
            }

            impl $name {
                pub fn from_ident(ident: &str) -> Result<$name, ()> {
                    match_ignore_ascii_case! { ident,
                        $($css => Ok($name::$variant),)+
                        _ => Err(())
                    }
                }
            }
        }
    }
    define_css_keyword_enum! {
        pub enum UserZoom {
            Zoom = "zoom",
            Fixed = "fixed",
        }
    }

    assert_eq!(UserZoom::from_ident("fixed"), Ok(UserZoom::Fixed));
}
