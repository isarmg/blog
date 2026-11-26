use super::*;

#[test]
fn invalid_prop_names_return_errors_for_all_patterns() {
    for name in [
        "",
        "bad name",
        "a::b",
        "1first",
        "type",
        " value ",
        "value/*comment*/",
        "value//comment",
        "value\u{200e}",
        "value\u{200f}",
        "\u{feff}value",
    ] {
        for pattern in ["value", "(value,)"] {
            let arg: FnArg =
                syn::parse_str(&format!("#[prop(name = {name:?})] {pattern}: String")).unwrap();
            let error = Prop::new(arg)
                .err()
                .expect("invalid rename must be rejected");
            assert!(
                error.to_string().contains("valid Rust identifier"),
                "{error}"
            );
            assert!(
                error
                    .to_compile_error()
                    .to_string()
                    .contains("valid Rust identifier")
            );
        }
    }
}

#[test]
fn ordinary_unicode_and_raw_prop_names_remain_supported() {
    for name in ["renamed", "donnée", "e\u{301}", "r#type"] {
        let arg: FnArg =
            syn::parse_str(&format!("#[prop(name = {name:?})] value: String")).unwrap();
        let prop = Prop::new(arg).unwrap();
        assert_eq!(prop.name.ident.to_string(), name);
    }
}

#[test]
fn disabled_prop_flags_are_not_conflicts_but_duplicate_keys_are() {
    let parse = |options: &str| {
        let arg = syn::parse_str(&format!("#[prop({options})] field: Option<u8>")).unwrap();
        Prop::new(arg).map(|prop| prop.options)
    };
    let options = parse("optional=false, optional_no_strip, marker=false").unwrap();
    assert_eq!(options.mode, PropMode::OptionalNoStrip);
    for (options, diagnostic) in [
        ("optional, strip_option", "conflicting prop options"),
        ("marker, default=None", "conflicting prop options"),
        ("marker=false, marker", "duplicate prop option"),
    ] {
        let error = parse(options).unwrap_err();
        assert!(error.to_string().contains(diagnostic), "{error}");
    }
}
