const NUMERIC_TYPES: [&str; 14] = [
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64",
];

/// Compiles and executes one isolated JAHE program. Each generated executable runs as a child
/// process so an access violation or trap is captured as a test result instead of killing the
/// Rust test runner.
#[allow(dead_code)]
pub fn run_native_case(case: &str, source: &str) -> Result<std::process::Output, String> {
    use jahe_compiler::{backend::native::NativeBackend, frontend, ir};
    use std::{fs, process::Command};

    let module = frontend::lower_to_ir(source).map_err(|error| format!("frontend: {error:?}"))?;
    let verified = ir::verify::Verifier
        .verify(&module)
        .map_err(|error| format!("IR verifier: {error:?}"))?;
    let safe_case = case
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let root = std::env::temp_dir().join(format!(
        "jahe_native_case_{}_{}",
        std::process::id(),
        safe_case
    ));
    fs::create_dir_all(&root).map_err(|error| format!("create test directory: {error}"))?;
    let object = root.join("case.obj");
    let executable = root.join("case.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .map_err(|error| format!("native build/link: {error}"))?;
    if !object.is_file() || !executable.is_file() {
        return Err("native pipeline did not produce both the COFF object and executable".into());
    }
    Command::new(&executable)
        .output()
        .map_err(|error| format!("launch generated executable: {error}"))
}

/// Generates a single source program that validates constant-folded expected values against
/// ordinary typed IR execution for every same-type scalar operator and numeric cast pair.
#[allow(dead_code)]
pub fn scalar_operator_cast_matrix() -> String {
    scalar_operator_cast_matrix_with_native_wide_float_casts(true)
}

fn scalar_operator_cast_matrix_with_native_wide_float_casts(
    include_native_unsupported_casts: bool,
) -> String {
    let mut source = String::from("module scalar_operator_cast_matrix;\n");
    let mut checks = Vec::<(String, String)>::new();
    let integer_ops = [
        ("ADD", "+", "A", "B", false),
        ("SUB", "-", "A", "B", false),
        ("MUL", "*", "A", "B", false),
        ("DIV", "/", "A", "B", false),
        ("REM", "%", "A", "B", false),
        ("AND", "&", "A", "B", false),
        ("OR", "|", "A", "B", false),
        ("XOR", "^", "A", "B", false),
        ("SHL", "<<", "A", "ONE", false),
        ("SHR", ">>", "A", "ONE", false),
    ];
    let comparisons = [
        ("EQ", "==", false),
        ("NE", "!=", true),
        ("LT", "<", false),
        ("LE", "<=", false),
        ("GT", ">", true),
        ("GE", ">=", true),
    ];

    for (index, ty) in NUMERIC_TYPES.iter().enumerate() {
        if !include_native_unsupported_casts && matches!(*ty, "i128" | "u128") {
            continue;
        }
        let float = *ty == "f32" || *ty == "f64";
        let a = if float { "6.0" } else { "6" };
        let b = if float { "3.0" } else { "3" };
        let one = if float { "1.0" } else { "1" };
        let zero = if float { "0.0" } else { "0" };
        source.push_str(&format!(
            "const A{index}: {ty} = {a}; const B{index}: {ty} = {b}; const ONE{index}: {ty} = {one}; const ZERO{index}: {ty} = {zero};\n"
        ));

        let ops: Vec<(&str, &str, &str, &str, bool)> = if float {
            vec![
                ("ADD", "+", "A", "B", false),
                ("SUB", "-", "A", "B", false),
                ("MUL", "*", "A", "B", false),
                ("DIV", "/", "A", "B", false),
            ]
        } else {
            integer_ops.to_vec()
        };
        for (tag, op, lhs, rhs, _) in ops {
            let result_name = format!("E{tag}{index}");
            source.push_str(&format!(
                "const {result_name}: {ty} = {lhs}{index} {op} {rhs}{index};\n"
            ));
            checks.push((
                format!("check_{result_name}"),
                format!("if ((A{index} {op} {rhs}{index}) != {result_name}) {{ return 1; }}"),
            ));
        }

        for (tag, op, expected) in comparisons {
            let expected = if expected { "true" } else { "false" };
            let right = format!("B{index}");
            let result_name = format!("CMP{tag}{index}");
            source.push_str(&format!(
                "const {result_name}: bool = A{index} {op} {right};\n"
            ));
            checks.push((format!("check_{result_name}"), format!("if ((A{index} {op} {right}) != {expected} || (A{index} {op} {right}) != {result_name}) {{ return 2; }}")));
            let _ = expected;
        }

        if ty.starts_with('i') || *ty == "isize" {
            let neg_name = format!("NEG{index}");
            source.push_str(&format!("const {neg_name}: {ty} = -A{index};\n"));
            checks.push((
                format!("check_{neg_name}"),
                format!("if ((-A{index}) != {neg_name}) {{ return 3; }}"),
            ));
        } else if float {
            let neg_name = format!("NEG{index}");
            source.push_str(&format!("const {neg_name}: {ty} = -A{index};\n"));
            checks.push((
                format!("check_{neg_name}"),
                format!("if ((-A{index}) != {neg_name}) {{ return 3; }}"),
            ));
        } else {
            let neg_name = format!("NEGZERO{index}");
            source.push_str(&format!("const {neg_name}: {ty} = -ZERO{index};\n"));
            checks.push((
                format!("check_{neg_name}"),
                format!("if ((-ZERO{index}) != {neg_name}) {{ return 3; }}"),
            ));
        }

        for (target_index, target) in NUMERIC_TYPES.iter().enumerate() {
            if !include_native_unsupported_casts
                && ((matches!(*ty, "i128" | "u128") && matches!(*target, "f32" | "f64"))
                    || (matches!(*ty, "f32" | "f64") && matches!(*target, "i128" | "u128")))
                || (!include_native_unsupported_casts && matches!(*target, "i128" | "u128"))
            {
                continue;
            }
            let value = if float {
                format!("A{index}")
            } else {
                format!("A{index}")
            };
            let constant = format!("CAST{index}_{target_index}");
            source.push_str(&format!(
                "const {constant}: {target} = {value} as {target};\n"
            ));
            checks.push((
                format!("check_{constant}"),
                format!("if (({value} as {target}) != {constant}) {{ return 4; }}"),
            ));
        }
    }

    source.push_str("const BOOL_A: bool = true; const BOOL_B: bool = false; const BOOL_NOT: bool = !BOOL_A; const BOOL_EQ: bool = BOOL_A == BOOL_B; const BOOL_NE: bool = BOOL_A != BOOL_B;\n");
    source.push_str("const CHAR_A: char = 'é'; const CHAR_B: char = '\\u{E9}'; const CHAR_C: char = 'z'; const CHAR_EQ: bool = CHAR_A == CHAR_B; const CHAR_NE: bool = CHAR_A != CHAR_C;\n");
    source.push_str("const STR_A: str = \"jahe\"; const STR_B: str = \"jahe\"; const STR_C: str = \"other\"; const STR_EQ: bool = STR_A == STR_B; const STR_NE: bool = STR_A != STR_C;\n");
    let mut misc_checks = vec![
        ("check_bool_not".to_string(), "if ((!BOOL_A) != BOOL_NOT) { return 5; }".to_string()),
        ("check_bool_cmp".to_string(), "if ((BOOL_A == BOOL_B) != false || (BOOL_A != BOOL_B) != true || (BOOL_A == BOOL_B) != BOOL_EQ || (BOOL_A != BOOL_B) != BOOL_NE) { return 5; }".to_string()),
        ("check_char_cmp".to_string(), "if ((CHAR_A == CHAR_B) != true || (CHAR_A != CHAR_C) != true || (CHAR_A == CHAR_B) != CHAR_EQ || (CHAR_A != CHAR_C) != CHAR_NE) { return 6; }".to_string()),
    ];
    if include_native_unsupported_casts {
        misc_checks.push(("check_str_cmp".to_string(), "if ((STR_A == STR_B) != true || (STR_A != STR_C) != true || (STR_A == STR_B) != STR_EQ || (STR_A != STR_C) != STR_NE) { return 7; }".to_string()));
    }
    for (name, check) in &checks {
        source.push_str(&format!("func {name}() -> i32 {{ {check} return 0; }}\n"));
    }
    for (name, check) in &misc_checks {
        source.push_str(&format!("func {name}() -> i32 {{ {check} return 0; }}\n"));
    }
    let names = checks
        .iter()
        .map(|(name, _)| name.as_str())
        .chain(misc_checks.iter().map(|(name, _)| name.as_str()))
        .collect::<Vec<_>>();
    let mut dispatchers = Vec::new();
    for (group_index, group) in names.chunks(8).enumerate() {
        let dispatcher = format!("dispatch_{group_index}");
        source.push_str(&format!("func {dispatcher}() -> i32 {{\n"));
        for name in group {
            source.push_str(&format!("if ({name}()) != 0 {{ return 1; }}\n"));
        }
        source.push_str("return 0; }\n");
        dispatchers.push(dispatcher);
    }
    source.push_str("func main() -> i32 {\n");
    for dispatcher in dispatchers {
        source.push_str(&format!("if ({dispatcher}()) != 0 {{ return 1; }}\n"));
    }
    source.push_str("return 0; }\n");
    source
}

#[allow(dead_code)]
pub fn numeric_types() -> &'static [&'static str] {
    &NUMERIC_TYPES
}
