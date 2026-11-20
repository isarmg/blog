use crate::regular_expression::ast::{
    CharacterClass, CharacterClassContents, Disjunction, LookAroundAssertionKind, Pattern, Term,
};

/// Whether it is safe to fold a RegExp operation without assuming newer engine support.
pub fn is_regexp_syntax_supported(pattern: &Pattern<'_>, flags: &str) -> bool {
    let mut seen = 0_u8;
    for flag in flags.bytes() {
        let bit = match flag {
            b'g' => 1 << 0,
            b'i' => 1 << 1,
            b'm' => 1 << 2,
            _ => return false,
        };
        if seen & bit != 0 {
            return false;
        }
        seen |= bit;
    }
    !disjunction_contains_unsupported(&pattern.body)
}

fn disjunction_contains_unsupported(disjunction: &Disjunction) -> bool {
    disjunction
        .body
        .iter()
        .any(|alternative| alternative.body.iter().any(term_contains_unsupported))
}

fn term_contains_unsupported(term: &Term) -> bool {
    match term {
        Term::LookAroundAssertion(assertion) => {
            matches!(
                assertion.kind,
                LookAroundAssertionKind::Lookbehind | LookAroundAssertionKind::NegativeLookbehind
            ) || disjunction_contains_unsupported(&assertion.body)
        }
        Term::Quantifier(quantifier) => term_contains_unsupported(&quantifier.body),
        Term::UnicodePropertyEscape(_) => true,
        Term::CharacterClass(character_class) => {
            character_class_has_unicode_property_escape(character_class)
        }
        Term::CapturingGroup(group) => {
            group.name.is_some() || disjunction_contains_unsupported(&group.body)
        }
        Term::IgnoreGroup(group) => {
            group.modifiers.is_some() || disjunction_contains_unsupported(&group.body)
        }
        _ => false,
    }
}

fn character_class_has_unicode_property_escape(character_class: &CharacterClass) -> bool {
    character_class.body.iter().any(|element| match element {
        CharacterClassContents::UnicodePropertyEscape(_) => true,
        CharacterClassContents::NestedCharacterClass(character_class) => {
            character_class_has_unicode_property_escape(character_class)
        }
        _ => false,
    })
}

#[path = "../../../../../../../../test/foundation/assets/compiler/unit/parts/regular_expression/src/ast_impl/support/test.rs"]
#[cfg(test)]
mod test;
