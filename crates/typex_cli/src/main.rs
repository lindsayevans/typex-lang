use std::env;
use std::fs;
use std::process;
use typex_parser::parse;
use typex_span::SourceMap;

mod pretty;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: tx <command> [file]");
        eprintln!("Commands:");
        eprintln!("  build <file> [-o <output>]  Compile a .tx file to a native binary");
        eprintln!("  run <file>    Execute a .tx file");
        eprintln!("  repl          Start an interactive TypeX session");
        eprintln!("  ast <file>    Parse and print the AST of a .tx file");
        eprintln!("  check <file>  Parse, resolve and typecheck a .tx file");
        process::exit(1);
    }

    match args[1].as_str() {
        "build" => {
            if args.len() < 3 {
                eprintln!("Usage: tx build <file> [-o <output>]");
                process::exit(1);
            }
            let file = &args[2];
            let output = parse_output_flag(&args[3..]);
            cmd_build(file, output.as_deref());
        }
        "run" => {
            if args.len() < 3 {
                eprintln!("Usage: tx run <file>");
                process::exit(1);
            }
            cmd_run(&args[2], &args[2..]);
        }
        "repl" => {
            cmd_repl();
        }
        "ast" => {
            if args.len() < 3 {
                eprintln!("Usage: tx ast <file>");
                process::exit(1);
            }
            cmd_ast(&args[2]);
        }
        "check" => {
            if args.len() < 3 {
                eprintln!("Usage: tx check <file>");
                process::exit(1);
            }
            cmd_check(&args[2]);
        }
        other => {
            eprintln!("Unknown command: {}", other);
            process::exit(1);
        }
    }
}

fn cmd_build(path: &str, output: Option<&str>) {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {}", path, e);
            process::exit(1);
        }
    };

    let mut sm = SourceMap::new();
    let file = sm.add(path.to_string(), src.clone());

    // Parse
    let (module, parse_diags) = parse(&src, file);
    if print_diagnostics(&parse_diags, &sm) {
        process::exit(1);
    }

    // Resolve
    let resolve_diags = typex_resolve::resolve(&module);
    if print_diagnostics(&resolve_diags, &sm) {
        process::exit(1);
    }

    // Typecheck
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
        process::exit(1);
    }

    // Determine output paths
    let bin_path = if let Some(out) = output {
        out.to_string()
    } else {
        let file_stem = std::path::Path::new(path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let out_dir = "out";
        std::fs::create_dir_all(out_dir).ok();
        format!("{}/{}", out_dir, file_stem)
    };

    let obj_path = format!("{}.o", bin_path);
    if let Some(parent) = std::path::Path::new(&obj_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // Codegen
    println!("Compiling {}...", path);
    let obj_bytes = match typex_codegen::compile(&module, None) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Codegen error: {}", e);
            process::exit(1);
        }
    };

    // Write object file
    if let Err(e) = std::fs::write(&obj_path, &obj_bytes) {
        eprintln!("Failed to write object file: {}", e);
        process::exit(1);
    }

    // Compile runtime shim
    let shim_path = "/tmp/txruntime.c";
    let shim_obj = "/tmp/txruntime.o";
    std::fs::write(
        shim_path,
        include_str!("../../typex_codegen/runtime/txruntime.c"),
    )
    .expect("failed to write runtime shim");

    let shim_status = std::process::Command::new("cc")
        .args(["-c", shim_path, "-o", shim_obj])
        .status();

    if let Err(e) = shim_status {
        eprintln!("Failed to compile runtime shim: {}", e);
        process::exit(1);
    }

    // Link
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
            process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to run linker: {}", e);
            process::exit(1);
        }
    }
}

fn cmd_run(path: &str, argv: &[String]) {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {}", path, e);
            process::exit(1);
        }
    };

    let mut sm = SourceMap::new();
    let file = sm.add(path.to_string(), src.clone());

    // Parse
    let (module, parse_diags) = parse(&src, file);
    if print_diagnostics(&parse_diags, &sm) {
        process::exit(1);
    }

    // Resolve
    let resolve_diags = typex_resolve::resolve(&module);
    if print_diagnostics(&resolve_diags, &sm) {
        process::exit(1);
    }

    // Typecheck
    let type_diags = typex_typecheck::typecheck(&module);
    if print_diagnostics(&type_diags, &sm) {
        process::exit(1);
    }

    // Run
    let argv: Vec<String> = argv.to_vec();
    match typex_vm::run_with_path(&module, argv, path) {
        Ok(code) => process::exit(code as i32),
        Err(e) => {
            eprintln!("runtime error: {}", e.message);
            process::exit(1);
        }
    }
}

fn cmd_repl() {
    use std::io::{self, BufRead, Write};

    println!("TypeX REPL v0.1.0");
    println!("Type 'exit' or Ctrl+C to quit");
    println!();

    let mut sm = SourceMap::new();
    let stdin = io::stdin();
    let mut history: Vec<String> = Vec::new();

    loop {
        // prompt
        print!("tx> ");
        io::stdout().flush().unwrap();

        // read line
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }

        let line = line.trim().to_string();

        if line.is_empty() {
            continue;
        }

        if line == "exit" || line == "quit" {
            println!("Goodbye!");
            break;
        }

        // special REPL commands
        if line == ":help" {
            println!("Commands:");
            println!("  :help      Show this help");
            println!("  :history   Show input history");
            println!("  :clear     Clear history and reset state");
            println!("  exit       Exit the REPL");
            continue;
        }

        if line == ":history" {
            for (i, h) in history.iter().enumerate() {
                println!("{:3}: {}", i + 1, h);
            }
            continue;
        }

        if line == ":clear" {
            history.clear();
            println!("State cleared.");
            continue;
        }

        // wrap input in a main function to evaluate it
        // if it looks like a declaration, add it to history
        // if it looks like an expression, wrap it in println
        let is_decl = line.starts_with("function ")
            || line.starts_with("const ")
            || line.starts_with("let ")
            || line.starts_with("type ")
            || line.starts_with("enum ")
            || line.starts_with("import ");

        let src = if is_decl
            && (line.starts_with("function ")
                || line.starts_with("type ")
                || line.starts_with("enum ")
                || line.starts_with("import "))
        {
            // functions/types go at top level
            let mut full = history.join("\n");
            full.push('\n');
            full.push_str(&line);
            full.push_str("\nfunction main(): int { return 0; }");
            full
        } else {
            // const/let and expressions go inside main
            let mut full = history
                .iter()
                .filter(|h| {
                    h.starts_with("function ")
                        || h.starts_with("type ")
                        || h.starts_with("enum ")
                        || h.starts_with("import ")
                })
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");

            full.push('\n');

            // gather all const/let history into main body
            let inner: String = history
                .iter()
                .filter(|h| h.starts_with("const ") || h.starts_with("let "))
                .cloned()
                .collect::<Vec<_>>()
                .join("\n    ");

            let stmt = if line.ends_with(';') {
                line.clone()
            } else {
                format!("{};", line)
            };

            let body = if !inner.is_empty() {
                format!("    {}\n    {}", inner, stmt)
            } else {
                format!("    {}", stmt)
            };

            full.push_str(&format!(
                "\nfunction main(): int {{\n{}\n    return 0;\n}}",
                body
            ));
            full
        };

        // parse
        let file = sm.add(format!("repl:{}", history.len() + 1), src.clone());
        let (module, parse_diags) = parse(&src, file);

        if !parse_diags.is_empty() {
            for diag in &parse_diags {
                eprint!("{}", sm.render_diagnostic(diag));
            }
            continue;
        }

        // resolve
        let resolve_diags = typex_resolve::resolve(&module);
        if !resolve_diags.is_empty() {
            for diag in &resolve_diags {
                eprint!("{}", sm.render_diagnostic(diag));
            }
            continue;
        }

        // typecheck - show warnings but don't stop
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
            continue;
        }

        // run __repl_main__
        match typex_vm::run(&module, vec!["repl".to_string()]) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("runtime error: {}", e.message);
                continue;
            }
        }

        // if successful declaration, add to history
        if is_decl {
            history.push(line);
        }
    }
}

fn cmd_ast(path: &str) {
    // Read source file
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {}", path, e);
            process::exit(1);
        }
    };

    // Register in source map
    let mut sm = SourceMap::new();
    let file = sm.add(path.to_string(), src.clone());

    // Parse
    let (module, diagnostics) = parse(&src, file);

    // Print any diagnostics
    if !diagnostics.is_empty() {
        for diag in &diagnostics {
            let level = match diag.level {
                typex_span::Level::Error => "error",
                typex_span::Level::Warning => "warning",
                typex_span::Level::Note => "note",
            };
            eprintln!(
                "[{}] {}:{} — {}",
                level, path, diag.span.start.line, diag.message
            );
        }
        if diagnostics
            .iter()
            .any(|d| d.level == typex_span::Level::Error)
        {
            process::exit(1);
        }
    }

    // Pretty print AST
    println!("=== AST: {} ===\n", path);
    pretty::print_module(&module, 0);
}

fn cmd_check(path: &str) {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {}", path, e);
            process::exit(1);
        }
    };

    let mut sm = SourceMap::new();
    let file = sm.add(path.to_string(), src.clone());

    // Parse
    let (module, parse_diags) = parse(&src, file);
    let mut has_errors = print_diagnostics(&parse_diags, &sm);

    // Resolve
    let resolve_diags = typex_resolve::resolve(&module);
    has_errors |= print_diagnostics(&resolve_diags, &sm);

    // Typecheck
    let type_diags = typex_typecheck::typecheck(&module);
    has_errors |= print_diagnostics(&type_diags, &sm);

    if has_errors {
        process::exit(1);
    }

    println!("{}: ok", path);
}

fn print_diagnostics(diagnostics: &[typex_span::Diagnostic], sm: &typex_span::SourceMap) -> bool {
    let mut has_errors = false;
    for diag in diagnostics {
        eprint!("{}", sm.render_diagnostic(diag));
        if diag.level == typex_span::Level::Error {
            has_errors = true;
        }
    }
    has_errors
}

fn parse_output_flag(args: &[String]) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                if i + 1 < args.len() {
                    return Some(args[i + 1].clone());
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}
