use typex_ast::*;

pub fn print_module(module: &Module, indent: usize) {
    println!("{}Module ({} items)", ind(indent), module.items.len());
    for item in &module.items {
        print_item(item, indent + 1);
    }
}

pub fn print_item(item: &Item, indent: usize) {
    match item {
        Item::Function(f) => print_function(f, indent),
        Item::TypeAlias(t) => print_type_alias(t, indent),
        Item::Enum(e) => print_enum(e, indent),
        Item::Import(i) => print_import(i, indent),
        Item::Export(e) => print_export(e, indent),
        Item::Const(c) => print_const(c, indent),
        Item::Let(l) => print_let(l, indent),
    }
}

fn print_function(f: &FunctionDef, indent: usize) {
    let ret = f
        .return_type
        .as_ref()
        .map(|t| format!(": {}", type_expr_str(t)))
        .unwrap_or_default();
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name.name, type_expr_str(&p.ty)))
        .collect();
    println!(
        "{}Function {}({}){}",
        ind(indent),
        f.name.name,
        params.join(", "),
        ret
    );
    print_block(&f.body, indent + 1);
}

fn print_block(block: &Block, indent: usize) {
    println!("{}Block ({} stmts)", ind(indent), block.stmts.len());
    for stmt in &block.stmts {
        print_stmt(stmt, indent + 1);
    }
}

fn print_stmt(stmt: &Stmt, indent: usize) {
    match stmt {
        Stmt::Let(l) => print_let(l, indent),
        Stmt::Const(c) => print_const(c, indent),
        Stmt::Return(e, _) => {
            println!("{}Return", ind(indent));
            if let Some(expr) = e {
                print_expr(expr, indent + 1);
            }
        }
        Stmt::Expr(e) => print_expr(e, indent),
        Stmt::If(i) => print_if(i, indent),
        Stmt::Switch(s) => print_switch(s, indent),
        Stmt::For(f) => print_for(f, indent),
        Stmt::Match(m) => print_match(m, indent),
    }
}

fn print_let(l: &LetDef, indent: usize) {
    let ty =
        l.ty.as_ref()
            .map(|t| format!(": {}", type_expr_str(t)))
            .unwrap_or_default();
    println!("{}Let {}{}", ind(indent), l.name.name, ty);
    if let Some(val) = &l.value {
        print_expr(val, indent + 1);
    }
}

fn print_const(c: &ConstDef, indent: usize) {
    let ty =
        c.ty.as_ref()
            .map(|t| format!(": {}", type_expr_str(t)))
            .unwrap_or_default();
    println!("{}Const {}{}", ind(indent), c.name.name, ty);
    print_expr(&c.value, indent + 1);
}

fn print_if(i: &IfStmt, indent: usize) {
    println!("{}If", ind(indent));
    println!("{}  condition:", ind(indent));
    print_expr(&i.condition, indent + 2);
    println!("{}  then:", ind(indent));
    print_block(&i.then_block, indent + 2);
    for (cond, block) in &i.else_if {
        println!("{}  else if:", ind(indent));
        print_expr(cond, indent + 2);
        print_block(block, indent + 2);
    }
    if let Some(else_block) = &i.else_block {
        println!("{}  else:", ind(indent));
        print_block(else_block, indent + 2);
    }
}

fn print_switch(s: &SwitchStmt, indent: usize) {
    println!("{}Switch", ind(indent));
    print_expr(&s.value, indent + 1);
    for case in &s.cases {
        println!("{}  Case:", ind(indent));
        print_expr(&case.value, indent + 2);
        print_block(&case.body, indent + 2);
    }
    if let Some(default) = &s.default {
        println!("{}  Default:", ind(indent));
        print_block(default, indent + 2);
    }
}

fn print_for(f: &ForStmt, indent: usize) {
    match f {
        ForStmt::Array {
            index,
            value,
            iter,
            body,
            ..
        } => {
            println!(
                "{}For Array [index={}, value={}]",
                ind(indent),
                opt_name(index),
                opt_name(value)
            );
            print_expr(iter, indent + 1);
            print_block(body, indent + 1);
        }
        ForStmt::Object {
            key,
            value,
            iter,
            body,
            ..
        } => {
            println!(
                "{}For Object [key={}, value={}]",
                ind(indent),
                opt_name(key),
                opt_name(value)
            );
            print_expr(iter, indent + 1);
            print_block(body, indent + 1);
        }
        ForStmt::Str {
            index,
            offset,
            value,
            iter,
            body,
            ..
        } => {
            println!(
                "{}For String [index={}, offset={}, value={}]",
                ind(indent),
                opt_name(index),
                opt_name(offset),
                opt_name(value)
            );
            print_expr(iter, indent + 1);
            print_block(body, indent + 1);
        }
    }
}

fn print_match(m: &MatchExpr, indent: usize) {
    println!("{}Match", ind(indent));
    print_expr(&m.value, indent + 1);
    for arm in &m.arms {
        let pat = match &arm.pattern {
            Pattern::Ok(b) => format!("Ok({})", b.name),
            Pattern::Err(b) => format!("Err({})", b.name),
            Pattern::Wildcard => "_".to_string(),
            Pattern::EnumVariant(n, b) => match b {
                Some(b) => format!("{}({})", n.name, b.name),
                None => n.name.clone(),
            },
        };
        println!("{}  Arm: {}", ind(indent), pat);
        print_expr(&arm.body, indent + 2);
    }
}

fn print_expr(expr: &Expr, indent: usize) {
    match expr {
        Expr::Lit(lit) => match lit {
            Lit::Int(n, _) => println!("{}Int({})", ind(indent), n),
            Lit::Float(f, _) => println!("{}Float({})", ind(indent), f),
            Lit::Bool(b, _) => println!("{}Bool({})", ind(indent), b),
            Lit::Str(s, _) => println!("{}Str({:?})", ind(indent), s),
            Lit::Char(c, _) => println!("{}Char({:?})", ind(indent), c),
            Lit::Null(_) => println!("{}Null", ind(indent)),
        },
        Expr::Ident(i) => println!("{}Ident({})", ind(indent), i.name),
        Expr::BinOp(l, op, r, _) => {
            println!("{}BinOp({:?})", ind(indent), op);
            print_expr(l, indent + 1);
            print_expr(r, indent + 1);
        }
        Expr::UnaryOp(op, e, _) => {
            println!("{}UnaryOp({:?})", ind(indent), op);
            print_expr(e, indent + 1);
        }
        Expr::Call(f, args, _) => {
            println!("{}Call ({} args)", ind(indent), args.len());
            print_expr(f, indent + 1);
            for arg in args {
                print_expr(arg, indent + 1);
            }
        }
        Expr::Field(e, field, _) => {
            println!("{}Field .{}", ind(indent), field.name);
            print_expr(e, indent + 1);
        }
        Expr::Index(e, i, _) => {
            println!("{}Index", ind(indent));
            print_expr(e, indent + 1);
            print_expr(i, indent + 1);
        }
        Expr::Ternary(cond, then, else_, _) => {
            println!("{}Ternary", ind(indent));
            print_expr(cond, indent + 1);
            print_expr(then, indent + 1);
            print_expr(else_, indent + 1);
        }
        Expr::Match(m) => print_match(m, indent),
        Expr::Arrow(a) => {
            let params: Vec<String> = a
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name.name, type_expr_str(&p.ty)))
                .collect();
            println!("{}Arrow ({})", ind(indent), params.join(", "));
            match &a.body {
                ArrowBody::Expr(e) => print_expr(e, indent + 1),
                ArrowBody::Block(b) => print_block(b, indent + 1),
            }
        }
        Expr::Array(elements, _) => {
            println!("{}Array ({} elements)", ind(indent), elements.len());
            for e in elements {
                print_expr(e, indent + 1);
            }
        }
        Expr::Record(fields, _) => {
            println!("{}Record ({} fields)", ind(indent), fields.len());
            for (k, v) in fields {
                println!("{}  .{}", ind(indent), k.name);
                print_expr(v, indent + 1);
            }
        }
        Expr::Destructure(d) => match d {
            Destructure::Array(names, _) => {
                println!(
                    "{}Destructure Array [{}]",
                    ind(indent),
                    names
                        .iter()
                        .map(|n| n.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            Destructure::Object(fields, _) => {
                println!(
                    "{}Destructure Object [{}]",
                    ind(indent),
                    fields
                        .iter()
                        .map(|(k, _)| k.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        },
        Expr::Assign(lhs, rhs, _) => {
            println!("{}Assign", ind(indent));
            print_expr(lhs, indent + 1);
            print_expr(rhs, indent + 1);
        }
    }
}

fn print_type_alias(t: &TypeAlias, indent: usize) {
    println!(
        "{}TypeAlias {} = {}",
        ind(indent),
        t.name.name,
        type_expr_str(&t.ty)
    );
}

fn print_enum(e: &EnumDef, indent: usize) {
    println!("{}Enum {}", ind(indent), e.name.name);
    for v in &e.variants {
        let val = match &v.value {
            Some(EnumValue::Int(n)) => format!(" = {}", n),
            Some(EnumValue::Str(s)) => format!(" = {:?}", s),
            Some(EnumValue::Char(c)) => format!(" = {:?}", c),
            None => String::new(),
        };
        println!("{}  Variant {}{}", ind(indent), v.name.name, val);
    }
}

fn print_import(i: &Import, indent: usize) {
    let names: Vec<&str> = i.names.iter().map(|n| n.name.as_str()).collect();
    println!(
        "{}Import {{ {} }} from {:?}",
        ind(indent),
        names.join(", "),
        i.from
    );
}

fn print_export(e: &Export, indent: usize) {
    let names: Vec<&str> = e.names.iter().map(|n| n.name.as_str()).collect();
    println!("{}Export {{ {} }}", ind(indent), names.join(", "));
}

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

fn type_expr_str(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Named(n) => n.name.clone(),
        TypeExpr::Generic(n, args) => {
            let args: Vec<String> = args.iter().map(type_expr_str).collect();
            format!("{}<{}>", n.name, args.join(", "))
        }
        TypeExpr::Union(variants) => variants
            .iter()
            .map(type_expr_str)
            .collect::<Vec<_>>()
            .join(" | "),
        TypeExpr::Nullable(inner) => format!("{} | null", type_expr_str(inner)),
    }
}

fn opt_name(ident: &Option<Ident>) -> &str {
    ident.as_ref().map(|i| i.name.as_str()).unwrap_or("_")
}

fn ind(indent: usize) -> String {
    "  ".repeat(indent)
}
