use blog_wasm_module_transform::ModuleConfig;
use wasm_encoder::{
    BlockType, Catch, CodeSection, Function, FunctionSection, Instruction, Module, TagKind,
    TagSection, TagType, TypeSection,
};
use wasmparser::{Parser, Payload, Validator, WasmFeatures};

fn operators(wasm: &[u8]) -> Vec<String> {
    Parser::new(0)
        .parse_all(wasm)
        .filter_map(|payload| match payload.unwrap() {
            Payload::CodeSectionEntry(body) => Some(
                body.get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(|operator| format!("{:?}", operator.unwrap()))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect()
}

fn assert_round_trip(instructions: &[Instruction<'_>]) {
    let mut types = TypeSection::new();
    types.ty().function([], []);
    let mut functions = FunctionSection::new();
    functions.function(0);
    let mut tags = TagSection::new();
    tags.tag(TagType {
        kind: TagKind::Exception,
        func_type_idx: 0,
    });
    let mut function = Function::new([]);
    for instruction in instructions {
        function.instruction(instruction);
    }
    let mut code = CodeSection::new();
    code.function(&function);
    let mut module = Module::new();
    module
        .section(&types)
        .section(&functions)
        .section(&tags)
        .section(&code);
    let original = module.finish();
    let mut parsed = ModuleConfig::new().parse(&original).unwrap();
    let emitted = parsed.emit_wasm();
    let features = WasmFeatures::default() | WasmFeatures::LEGACY_EXCEPTIONS;
    Validator::new_with_features(features)
        .validate_all(&emitted)
        .unwrap();
    assert_eq!(operators(&emitted), operators(&original));
}

#[test]
fn emits_browser_metadata_and_discards_embedded_dwarf() {
    let mut module = Module::new();
    let mut names = wasm_encoder::NameSection::new();
    names.module("browser");
    module
        .section(&names)
        .section(&wasm_encoder::CustomSection {
            name: ".debug_info".into(),
            data: [0xff, 0xff].as_slice().into(),
        })
        .section(&wasm_encoder::CustomSection {
            name: "blog-metadata".into(),
            data: [1, 2, 3].as_slice().into(),
        });

    let mut parsed = ModuleConfig::new().parse(&module.finish()).unwrap();
    assert_eq!(parsed.name.as_deref(), Some("browser"));
    let emitted = parsed.emit_wasm();
    Validator::new().validate_all(&emitted).unwrap();

    let custom_sections: Vec<_> = Parser::new(0)
        .parse_all(&emitted)
        .filter_map(|payload| match payload.unwrap() {
            Payload::CustomSection(section) => Some((section.name(), section.data())),
            _ => None,
        })
        .collect();
    assert!(custom_sections.iter().any(|(name, _)| *name == "name"));
    assert!(custom_sections.iter().any(|(name, _)| *name == "producers"));
    assert!(custom_sections.contains(&("blog-metadata", &[1, 2, 3])));
    assert!(custom_sections
        .iter()
        .all(|(name, _)| !name.starts_with(".debug")));
}

#[test]
fn try_table_preserves_empty_and_nested_catches() {
    assert_round_trip(&[
        Instruction::Block(BlockType::Empty),
        Instruction::TryTable(
            BlockType::Empty,
            vec![Catch::One { tag: 0, label: 0 }, Catch::All { label: 0 }].into(),
        ),
        Instruction::TryTable(BlockType::Empty, vec![].into()),
        Instruction::I32Const(7),
        Instruction::Drop,
        Instruction::End,
        Instruction::End,
        Instruction::End,
        Instruction::End,
    ]);
}

#[test]
fn legacy_try_preserves_tagged_and_catch_all_handlers() {
    assert_round_trip(&[
        Instruction::Try(BlockType::Empty),
        Instruction::I32Const(7),
        Instruction::Drop,
        Instruction::Catch(0),
        Instruction::I32Const(8),
        Instruction::Drop,
        Instruction::CatchAll,
        Instruction::I32Const(9),
        Instruction::Drop,
        Instruction::End,
        Instruction::End,
    ]);
}

#[test]
fn legacy_try_preserves_delegation() {
    assert_round_trip(&[
        Instruction::Block(BlockType::Empty),
        Instruction::Try(BlockType::Empty),
        Instruction::I32Const(7),
        Instruction::Drop,
        Instruction::Delegate(0),
        Instruction::End,
        Instruction::End,
    ]);
}
