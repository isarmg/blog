use crate::error::{Error, Result};
use proc_macro::{Delimiter, Ident, Span, TokenTree, token_stream};
use std::{env::VarError, iter::Peekable};

pub(crate) enum Segment {
    String(LitStr),
    Apostrophe(Span),
    Env(LitStr),
    Modifier(Span, Ident),
}

pub(crate) struct LitStr {
    pub value: String,
    pub span: Span,
}

pub(crate) fn parse(tokens: &mut Peekable<token_stream::IntoIter>) -> Result<Vec<Segment>> {
    let mut segments = Vec::new();
    while match tokens.peek() {
        None => false,
        Some(TokenTree::Punct(punct)) => punct.as_char() != '>',
        Some(_) => true,
    } {
        match tokens.next().expect("invariant: peek saw a token") {
            TokenTree::Ident(ident) => {
                let mut fragment = ident.to_string();
                if fragment.starts_with("r#") {
                    fragment = fragment.split_off(2);
                }
                if fragment == "env"
                    && let Some(bang) = tokens.next_if(
                        |token| matches!(token, TokenTree::Punct(punct) if punct.as_char() == '!'),
                    )
                {
                    segments.push(parse_env(tokens, ident.span(), bang.span())?);
                } else {
                    segments.push(Segment::String(LitStr {
                        value: fragment,
                        span: ident.span(),
                    }));
                }
            }
            TokenTree::Literal(lit) => {
                segments.push(Segment::String(LitStr {
                    value: lit.to_string(),
                    span: lit.span(),
                }));
            }
            TokenTree::Punct(punct) => match punct.as_char() {
                '_' => segments.push(Segment::String(LitStr {
                    value: "_".to_owned(),
                    span: punct.span(),
                })),
                '\'' => segments.push(Segment::Apostrophe(punct.span())),
                ':' => {
                    let colon_span = punct.span();
                    let ident = match tokens.next() {
                        Some(TokenTree::Ident(ident)) => ident,
                        wrong => {
                            let span = wrong.as_ref().map_or(colon_span, TokenTree::span);
                            return Err(Error::new(span, "expected identifier after `:`"));
                        }
                    };
                    segments.push(Segment::Modifier(colon_span, ident));
                }
                '#' => segments.push(Segment::String(LitStr {
                    value: "#".to_string(),
                    span: punct.span(),
                })),
                _ => return Err(Error::new(punct.span(), "unexpected punct")),
            },
            TokenTree::Group(group) => {
                if group.delimiter() == Delimiter::None {
                    let mut inner = group.stream().into_iter().peekable();
                    let nested = parse(&mut inner)?;
                    if let Some(unexpected) = inner.next() {
                        return Err(Error::new(unexpected.span(), "unexpected token"));
                    }
                    segments.extend(nested);
                } else {
                    return Err(Error::new(group.span(), "unexpected token"));
                }
            }
        }
    }
    Ok(segments)
}

fn parse_env(
    tokens: &mut Peekable<token_stream::IntoIter>,
    env: Span,
    bang: Span,
) -> Result<Segment> {
    let args = match tokens.next() {
        Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Parenthesis => group,
        Some(wrong) => return Err(Error::new(wrong.span(), "expected `(`")),
        None => return Err(Error::new2(env, bang, "expected `(` after `env!`")),
    };
    let mut inner = args.stream().into_iter();
    let lit = match inner.next() {
        Some(TokenTree::Literal(lit)) => lit,
        Some(wrong) => return Err(Error::new(wrong.span(), "expected string literal")),
        None => {
            return Err(Error::new2(
                env,
                args.span(),
                "expected string literal as argument to env! macro",
            ));
        }
    };
    let repr = lit.to_string();
    let value = repr
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| Error::new(lit.span(), "expected string literal"))?;
    if let Some(unexpected) = inner.next() {
        return Err(Error::new(
            unexpected.span(),
            "unexpected token in env! macro",
        ));
    }
    Ok(Segment::Env(LitStr {
        value: value.to_owned(),
        span: lit.span(),
    }))
}

pub(crate) fn paste(segments: &[Segment]) -> Result<String> {
    let mut evaluated = Vec::new();
    let mut is_lifetime = false;

    for (i, segment) in segments.iter().enumerate() {
        match segment {
            Segment::String(segment) => {
                if segment.value.as_str() == "#" {
                    if i == 0 {
                        // Enable Raw mode
                        evaluated.push(String::from("r#"));
                        continue;
                    }
                    return Err(Error::new(
                        segment.span,
                        "`#` is reserved keyword and it enables the raw mode \
                            (i.e. generate Raw Identifiers) and it is only allowed in \
                            the beginning like `[< # ... >]`",
                    ));
                }
                evaluated.push(segment.value.clone());
            }
            Segment::Apostrophe(span) => {
                if is_lifetime {
                    return Err(Error::new(*span, "unexpected lifetime"));
                }
                is_lifetime = true;
            }
            Segment::Env(var) => {
                let resolved = std::env::var(&var.value).map_err(|error| {
                    Error::new(var.span, &environment_error_message(&var.value, &error))
                })?;
                let resolved = resolved.replace('-', "_");
                evaluated.push(resolved);
            }
            Segment::Modifier(colon, ident) => {
                let last = evaluated
                    .pop()
                    .ok_or_else(|| Error::new2(*colon, ident.span(), "unexpected modifier"))?;
                evaluated.push(apply_modifier(&last, *colon, ident)?);
            }
        }
    }

    let mut pasted = evaluated.into_iter().collect::<String>();
    if is_lifetime {
        pasted.insert(0, '\'');
    }
    Ok(pasted)
}

fn apply_modifier(text: &str, colon: Span, ident: &Ident) -> Result<String> {
    let modified = match ident.to_string().as_str() {
        "lower" => text.to_lowercase(),
        "upper" => text.to_uppercase(),
        "snake" => {
            let mut result = String::new();
            let mut prev = '_';
            for ch in text.chars() {
                if ch.is_uppercase() && prev != '_' {
                    result.push('_');
                }
                result.push(ch);
                prev = ch;
            }
            result.to_lowercase()
        }
        "camel" | "upper_camel" => camel_case(text, CamelCase::Upper),
        "lower_camel" => camel_case(text, CamelCase::Lower),
        "camel_edge" => camel_case(text, CamelCase::Edge),
        _ => return Err(Error::new2(colon, ident.span(), "unsupported modifier")),
    };
    Ok(modified)
}

#[derive(Clone, Copy)]
enum CamelCase {
    Upper,
    Lower,
    Edge,
}

fn camel_case(text: &str, style: CamelCase) -> String {
    let mut result = String::new();
    let mut prev = '_';
    let mut lower_first = matches!(style, CamelCase::Lower);
    for ch in text.chars() {
        if ch == '_' {
            if prev == '_' && matches!(style, CamelCase::Edge) {
                result.push(ch);
            }
        } else if prev == '_' {
            if lower_first {
                result.extend(ch.to_lowercase());
                lower_first = false;
            } else {
                result.extend(ch.to_uppercase());
            }
        } else if prev.is_uppercase() {
            result.extend(ch.to_lowercase());
        } else {
            result.push(ch);
        }
        prev = ch;
    }
    result
}

fn environment_error_message(name: &str, error: &VarError) -> String {
    match error {
        VarError::NotPresent => format!("environment variable {name:?} is not set"),
        VarError::NotUnicode(_) => format!("environment variable {name:?} is not valid Unicode"),
    }
}

#[cfg(test)]
#[path = "../../../../../test/foundation/syntax/token-concat/unit/segment.rs"]
mod tests;
