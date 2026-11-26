use token_concat::paste;

macro_rules! expand {
    ($name:ident, $event:ident) => {
        paste! {
            #[doc = "The `" [<$event>] "` event."]
            struct [<$name:camel>];

            impl [<$name:camel>] {
                fn [<on_ $event:lower>]() -> Self {
                    Self
                }
            }
        }
    };
}

expand!(aria_label, CLICK);

#[test]
fn dom_style_macro_emits_camel_case_types_and_lower_case_methods() {
    let _: AriaLabel = AriaLabel::on_click();
}

#[test]
fn style_compiler_item_macro_concatenates_a_prefix_and_number() {
    token_concat::item! {
        const [<ITEM_ 7>]: u8 = 7;
    }
    assert_eq!(ITEM_7, 7);
}
