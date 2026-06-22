use std::collections::HashMap;
use typex_runtime::{RuntimeError, RuntimeResult, Value};

// ------------------------------------------------------------------
// Stdlib module registry
// ------------------------------------------------------------------

pub type StdFn = fn(Vec<Value>) -> RuntimeResult<Value>;

#[derive(Clone)]
pub struct StdModule {
    pub functions: HashMap<String, StdFn>,
}

impl StdModule {
    fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }

    fn register(&mut self, name: &str, f: StdFn) {
        self.functions.insert(name.to_string(), f);
    }
}

pub struct StdRegistry {
    modules: HashMap<String, StdModule>,
}

impl StdRegistry {
    pub fn new() -> Self {
        let mut r = Self {
            modules: HashMap::new(),
        };
        r.modules
            .insert("tx:process".to_string(), process::module());
        r.modules.insert("tx:fs".to_string(), fs::module());
        r.modules.insert("tx:io".to_string(), io::module());
        r.modules.insert("tx:math".to_string(), math::module());
        r
    }

    pub fn get_fn(&self, module: &str, name: &str) -> Option<StdFn> {
        self.modules.get(module)?.functions.get(name).copied()
    }

    pub fn has_module(&self, module: &str) -> bool {
        self.modules.contains_key(module)
    }
}

impl Default for StdRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ------------------------------------------------------------------
// tx:fs
// ------------------------------------------------------------------

pub mod fs {
    use super::*;
    use std::fs;

    pub fn module() -> StdModule {
        let mut m = StdModule::new();
        m.register("readFile", read_file);
        m.register("writeFile", write_file);
        m.register("exists", exists);
        m.register("deleteFile", delete_file);
        m
    }

    fn read_file(args: Vec<Value>) -> RuntimeResult<Value> {
        let path = require_string(&args, 0, "readFile")?;
        match fs::read_to_string(&path) {
            Ok(content) => Ok(Value::Ok(Box::new(Value::Str(content)))),
            Err(e) => Ok(Value::Err(Box::new(Value::Str(e.to_string())))),
        }
    }

    fn write_file(args: Vec<Value>) -> RuntimeResult<Value> {
        let path = require_string(&args, 0, "writeFile")?;
        let content = require_string(&args, 1, "writeFile")?;
        match fs::write(&path, content) {
            Ok(_) => Ok(Value::Ok(Box::new(Value::Void))),
            Err(e) => Ok(Value::Err(Box::new(Value::Str(e.to_string())))),
        }
    }

    fn exists(args: Vec<Value>) -> RuntimeResult<Value> {
        let path = require_string(&args, 0, "exists")?;
        Ok(Value::Bool(std::path::Path::new(&path).exists()))
    }

    fn delete_file(args: Vec<Value>) -> RuntimeResult<Value> {
        let path = require_string(&args, 0, "deleteFile")?;
        match fs::remove_file(&path) {
            Ok(_) => Ok(Value::Ok(Box::new(Value::Void))),
            Err(e) => Ok(Value::Err(Box::new(Value::Str(e.to_string())))),
        }
    }
}

// ------------------------------------------------------------------
// tx:io
// ------------------------------------------------------------------

pub mod io {
    use super::*;
    use std::io::{self, BufRead, Write};

    pub fn module() -> StdModule {
        let mut m = StdModule::new();
        m.register("readLine", read_line);
        m.register("readLines", read_lines);
        m
    }

    fn read_line(args: Vec<Value>) -> RuntimeResult<Value> {
        // optional prompt
        if let Some(Value::Str(prompt)) = args.first() {
            print!("{}", prompt);
            io::stdout().flush().ok();
        }
        let mut line = String::new();
        match io::stdin().lock().read_line(&mut line) {
            Ok(_) => {
                // trim trailing newline
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(Value::Ok(Box::new(Value::Str(line))))
            }
            Err(e) => Ok(Value::Err(Box::new(Value::Str(e.to_string())))),
        }
    }

    fn read_lines(_args: Vec<Value>) -> RuntimeResult<Value> {
        let stdin = io::stdin();
        let lines: Vec<Value> = stdin
            .lock()
            .lines()
            .map(|l| match l {
                Ok(line) => Value::Str(line),
                Err(_) => Value::Str(String::new()),
            })
            .collect();
        Ok(Value::Array(lines))
    }
}

// ------------------------------------------------------------------
// tx:math
// ------------------------------------------------------------------

pub mod math {
    use super::*;

    pub fn module() -> StdModule {
        let mut m = StdModule::new();
        m.register("sqrt", sqrt);
        m.register("abs", abs);
        m.register("pow", pow);
        m.register("floor", floor);
        m.register("ceil", ceil);
        m.register("round", round);
        m.register("min", min);
        m.register("max", max);
        m.register("clamp", clamp);
        m
    }

    fn sqrt(args: Vec<Value>) -> RuntimeResult<Value> {
        let n = require_float(&args, 0, "sqrt")?;
        if n < 0.0 {
            return Ok(Value::Err(Box::new(Value::Str(
                "sqrt of negative number".to_string(),
            ))));
        }
        Ok(Value::Ok(Box::new(Value::Float(n.sqrt()))))
    }

    fn abs(args: Vec<Value>) -> RuntimeResult<Value> {
        match args.first() {
            Some(Value::Int(n)) => Ok(Value::Int(n.abs())),
            Some(Value::Float(f)) => Ok(Value::Float(f.abs())),
            _ => Err(RuntimeError::new("abs requires a numeric argument")),
        }
    }

    fn pow(args: Vec<Value>) -> RuntimeResult<Value> {
        let base = require_float(&args, 0, "pow")?;
        let exp = require_float(&args, 1, "pow")?;
        Ok(Value::Float(base.powf(exp)))
    }

    fn floor(args: Vec<Value>) -> RuntimeResult<Value> {
        let n = require_float(&args, 0, "floor")?;
        Ok(Value::Int(n.floor() as i64))
    }

    fn ceil(args: Vec<Value>) -> RuntimeResult<Value> {
        let n = require_float(&args, 0, "ceil")?;
        Ok(Value::Int(n.ceil() as i64))
    }

    fn round(args: Vec<Value>) -> RuntimeResult<Value> {
        let n = require_float(&args, 0, "round")?;
        Ok(Value::Int(n.round() as i64))
    }

    fn min(args: Vec<Value>) -> RuntimeResult<Value> {
        match (args.first(), args.get(1)) {
            (Some(Value::Int(a)), Some(Value::Int(b))) => Ok(Value::Int(*a.min(b))),
            (Some(Value::Float(a)), Some(Value::Float(b))) => Ok(Value::Float(a.min(*b))),
            _ => Err(RuntimeError::new("min requires two numeric arguments")),
        }
    }

    fn max(args: Vec<Value>) -> RuntimeResult<Value> {
        match (args.first(), args.get(1)) {
            (Some(Value::Int(a)), Some(Value::Int(b))) => Ok(Value::Int(*a.max(b))),
            (Some(Value::Float(a)), Some(Value::Float(b))) => Ok(Value::Float(a.max(*b))),
            _ => Err(RuntimeError::new("max requires two numeric arguments")),
        }
    }

    fn clamp(args: Vec<Value>) -> RuntimeResult<Value> {
        match (args.first(), args.get(1), args.get(2)) {
            (Some(Value::Int(n)), Some(Value::Int(lo)), Some(Value::Int(hi))) => {
                Ok(Value::Int((*n).clamp(*lo, *hi)))
            }
            (Some(Value::Float(n)), Some(Value::Float(lo)), Some(Value::Float(hi))) => {
                Ok(Value::Float(n.clamp(*lo, *hi)))
            }
            _ => Err(RuntimeError::new("clamp requires three numeric arguments")),
        }
    }
}

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

fn require_string(args: &[Value], idx: usize, fn_name: &str) -> RuntimeResult<String> {
    match args.get(idx) {
        Some(Value::Str(s)) => Ok(s.clone()),
        Some(other) => Err(RuntimeError::new(format!(
            "{}: argument {} must be a string, got {}",
            fn_name,
            idx + 1,
            other.type_name()
        ))),
        None => Err(RuntimeError::new(format!(
            "{}: missing argument {}",
            fn_name,
            idx + 1
        ))),
    }
}

fn require_float(args: &[Value], idx: usize, fn_name: &str) -> RuntimeResult<f64> {
    match args.get(idx) {
        Some(Value::Float(f)) => Ok(*f),
        Some(Value::Int(n)) => Ok(*n as f64),
        Some(other) => Err(RuntimeError::new(format!(
            "{}: argument {} must be numeric, got {}",
            fn_name,
            idx + 1,
            other.type_name()
        ))),
        None => Err(RuntimeError::new(format!(
            "{}: missing argument {}",
            fn_name,
            idx + 1
        ))),
    }
}

// ------------------------------------------------------------------
// tx:process
// ------------------------------------------------------------------

pub mod process {
    use super::*;
    use std::process::Command;

    pub fn module() -> StdModule {
        let mut m = StdModule::new();
        m.register("exec", exec);
        m.register("exit", exit);
        m
    }

    fn exec(args: Vec<Value>) -> RuntimeResult<Value> {
        let cmd = require_string(&args, 0, "exec")?;

        // split command into program + args
        let mut parts = cmd.split_whitespace();
        let program = match parts.next() {
            Some(p) => p,
            None => {
                return Ok(Value::Err(Box::new(Value::Str(
                    "empty command".to_string(),
                ))));
            }
        };
        let cmd_args: Vec<&str> = parts.collect();

        match Command::new(program).args(&cmd_args).output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let combined = if stderr.is_empty() {
                    stdout
                } else {
                    format!("{}{}", stdout, stderr)
                };
                if output.status.success() {
                    Ok(Value::Ok(Box::new(Value::Str(combined))))
                } else {
                    Ok(Value::Err(Box::new(Value::Str(combined))))
                }
            }
            Err(e) => Ok(Value::Err(Box::new(Value::Str(e.to_string())))),
        }
    }

    fn exit(args: Vec<Value>) -> RuntimeResult<Value> {
        let code = match args.first() {
            Some(Value::Int(n)) => *n as i32,
            _ => 0,
        };
        std::process::exit(code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(module: &str, func: &str, args: Vec<Value>) -> RuntimeResult<Value> {
        let registry = StdRegistry::new();
        let f = registry
            .get_fn(module, func)
            .expect(&format!("function '{}' not found in '{}'", func, module));
        f(args)
    }

    #[test]
    fn test_registry_has_modules() {
        let r = StdRegistry::new();
        assert!(r.has_module("tx:fs"));
        assert!(r.has_module("tx:io"));
        assert!(r.has_module("tx:math"));
        assert!(!r.has_module("tx:nonexistent"));
    }

    #[test]
    fn test_math_sqrt() {
        let result = call("tx:math", "sqrt", vec![Value::Float(9.0)]).unwrap();
        assert_eq!(result, Value::Ok(Box::new(Value::Float(3.0))));
    }

    #[test]
    fn test_math_sqrt_negative() {
        let result = call("tx:math", "sqrt", vec![Value::Float(-1.0)]).unwrap();
        assert!(matches!(result, Value::Err(_)));
    }

    #[test]
    fn test_math_abs_int() {
        let result = call("tx:math", "abs", vec![Value::Int(-42)]).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_math_abs_float() {
        let result = call("tx:math", "abs", vec![Value::Float(-3.14)]).unwrap();
        assert_eq!(result, Value::Float(3.14));
    }

    #[test]
    fn test_math_pow() {
        let result = call("tx:math", "pow", vec![Value::Float(2.0), Value::Float(8.0)]).unwrap();
        assert_eq!(result, Value::Float(256.0));
    }

    #[test]
    fn test_math_floor() {
        let result = call("tx:math", "floor", vec![Value::Float(3.7)]).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_math_ceil() {
        let result = call("tx:math", "ceil", vec![Value::Float(3.2)]).unwrap();
        assert_eq!(result, Value::Int(4));
    }

    #[test]
    fn test_math_round() {
        let result = call("tx:math", "round", vec![Value::Float(3.5)]).unwrap();
        assert_eq!(result, Value::Int(4));
    }

    #[test]
    fn test_math_min() {
        let result = call("tx:math", "min", vec![Value::Int(3), Value::Int(7)]).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_math_max() {
        let result = call("tx:math", "max", vec![Value::Int(3), Value::Int(7)]).unwrap();
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn test_math_clamp() {
        let result = call(
            "tx:math",
            "clamp",
            vec![Value::Int(15), Value::Int(0), Value::Int(10)],
        )
        .unwrap();
        assert_eq!(result, Value::Int(10));
    }

    #[test]
    fn test_fs_write_and_read() {
        let path = "/tmp/typex_test.txt";
        let content = "hello from typex!";

        let write_result = call(
            "tx:fs",
            "writeFile",
            vec![
                Value::Str(path.to_string()),
                Value::Str(content.to_string()),
            ],
        )
        .unwrap();
        assert!(matches!(write_result, Value::Ok(_)));

        let read_result = call("tx:fs", "readFile", vec![Value::Str(path.to_string())]).unwrap();
        assert_eq!(
            read_result,
            Value::Ok(Box::new(Value::Str(content.to_string())))
        );
    }

    #[test]
    fn test_fs_exists() {
        let result = call("tx:fs", "exists", vec![Value::Str("/tmp".to_string())]).unwrap();
        assert_eq!(result, Value::Bool(true));

        let result2 = call(
            "tx:fs",
            "exists",
            vec![Value::Str("/tmp/does_not_exist_typex".to_string())],
        )
        .unwrap();
        assert_eq!(result2, Value::Bool(false));
    }

    #[test]
    fn test_fs_read_missing_file() {
        let result = call(
            "tx:fs",
            "readFile",
            vec![Value::Str("/tmp/does_not_exist_typex.tx".to_string())],
        )
        .unwrap();
        assert!(matches!(result, Value::Err(_)));
    }

    #[test]
    fn test_process_exec_success() {
        let result = call(
            "tx:process",
            "exec",
            vec![Value::Str("echo hello".to_string())],
        )
        .unwrap();
        assert_eq!(
            result,
            Value::Ok(Box::new(Value::Str("hello\n".to_string())))
        );
    }

    #[test]
    fn test_process_exec_failure() {
        let result = call(
            "tx:process",
            "exec",
            vec![Value::Str("ls /nonexistent_path_typex".to_string())],
        )
        .unwrap();
        assert!(matches!(result, Value::Err(_)));
    }

    #[test]
    fn test_process_exec_with_args() {
        let result = call(
            "tx:process",
            "exec",
            vec![Value::Str("echo hello world".to_string())],
        )
        .unwrap();
        assert_eq!(
            result,
            Value::Ok(Box::new(Value::Str("hello world\n".to_string())))
        );
    }

    #[test]
    fn test_registry_has_process() {
        let r = StdRegistry::new();
        assert!(r.has_module("tx:process"));
    }
}
