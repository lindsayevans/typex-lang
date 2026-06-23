/// txs — TypeX script runner
/// Alias for `tx run <file> [args]`
fn main() {
    let mut args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: txs <file> [args]");
        eprintln!("       Alias for: tx run <file> [args]");
        std::process::exit(1);
    }

    // rewrite args as: tx run <file> [args]
    // args[0] = "txs", args[1] = file, args[2..] = script args
    // we want: ["tx", "run", file, args...]
    args[0] = "tx".to_string();
    args.insert(1, "run".to_string());

    // delegate to main tx binary logic by re-invoking with new args
    // since we share the same codebase, just call the run command directly
    let file = &args[2].clone();
    let argv = args[2..].to_vec();
    run_file(file, &argv);
}

fn run_file(path: &str, argv: &[String]) {
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

    match typex_vm::run_with_path(&module, argv.to_vec(), path) {
        Ok(code) => std::process::exit(code as i32),
        Err(e) => {
            eprintln!("runtime error: {}", e.message);
            std::process::exit(1);
        }
    }
}
