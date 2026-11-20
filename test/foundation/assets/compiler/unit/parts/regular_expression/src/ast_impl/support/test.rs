use super::*;
use crate::{
    regular_expression::{LiteralParser, Options},
    syntax_arena::Allocator,
};

#[test]
fn regular_expression_folding_preserves_feature_probes() {
    let allocator = Allocator::default();
    let parse = |source| {
        LiteralParser::new(&allocator, source, None, Options::default())
            .parse()
            .unwrap()
    };
    let baseline = parse("a(?=b)(?:c)+");
    assert!(is_regexp_syntax_supported(&baseline, "gim"));
    for flags in ["gg", "y", "u", "s", "d", "v", "x"] {
        assert!(!is_regexp_syntax_supported(&baseline, flags), "{flags}");
    }
    for source in ["(?<name>a)", "(?<name>a)|(?<name>b)", "(?<=a)b", "(?i:a)"] {
        assert!(!is_regexp_syntax_supported(&parse(source), ""), "{source}");
    }
}
