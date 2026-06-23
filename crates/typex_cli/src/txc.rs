/// txc — TypeX compiler
/// Alias for `tx build <file>`
fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: txc <file>");
        eprintln!("       Alias for: tx build <file>");
        std::process::exit(1);
    }

    let path = &args[1];
    build_file(path);
}

fn build_file(path: &str) {
    use std::fs;
    use typex_parser::parse;
    use typex_span::SourceMap;

    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {}", path, e);
            std::process::exit(1);
        }
    };

    let mut sm = SourceMap::new();
    let file = sm.add(path.to_string(), src.clone());

    let (module, parse_diags) = parse(&src, file);
    if !parse_diags.is_empty() {
        for diag in &parse_diags {
            eprint!("{}", sm.render_diagnostic(diag));
        }
        std::process::exit(1);
    }

    let resolve_diags = typex_resolve::resolve(&module);
    if !resolve_diags.is_empty() {
        for diag in &resolve_diags {
            eprint!("{}", sm.render_diagnostic(diag));
        }
        std::process::exit(1);
    }

    let type_diags = typex_typecheck::typecheck(&module);
    let has_errors = type_diags
        .iter()
        .any(|d| d.level == typex_span::Level::Error);
    for diag in &type_diags {
        if diag.level == typex_span::Level::Error {
            eprint!("{}", sm.render_diagnostic(diag));
        }
    }
    if has_errors {
        std::process::exit(1);
    }

    println!("Compiling {}...", path);
    let obj_bytes = match typex_codegen::compile(&module, None) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Codegen error: {}", e);
            std::process::exit(1);
        }
    };

    let obj_path = path.replace(".tx", ".o");
    if let Err(e) = std::fs::write(&obj_path, &obj_bytes) {
        eprintln!("Failed to write object file: {}", e);
        std::process::exit(1);
    }

    let bin_path = path.replace(".tx", "");

    let shim_path = "/tmp/txruntime.c";
    let shim_obj = "/tmp/txruntime.o";
    std::fs::write(
        shim_path,
        include_str!("../../typex_codegen/runtime/txruntime.c"),
    )
    .expect("failed to write runtime shim");

    std::process::Command::new("cc")
        .args(["-c", shim_path, "-o", shim_obj])
        .status()
        .ok();

    println!("Linking {}...", bin_path);
    let status = std::process::Command::new("cc")
        .arg(&obj_path)
        .arg(shim_obj)
        .arg("-o")
        .arg(&bin_path)
        .status();

    std::fs::remove_file(&obj_path).ok();

    match status {
        Ok(s) if s.success() => println!("Built: {}", bin_path),
        Ok(s) => {
            eprintln!("Linker failed: {}", s);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to run linker: {}", e);
            std::process::exit(1);
        }
    }
}
