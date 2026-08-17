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
        r.modules.insert("tx:env".to_string(), env::module());
        r.modules.insert("tx:time".to_string(), time::module());

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
// tx:env
// ------------------------------------------------------------------

pub mod env {
    use super::*;
    use std::env;

    pub fn module() -> StdModule {
        let mut m = StdModule::new();
        m.register("getenv", getenv);
        m.register("setenv", setenv);
        m.register("args", args);
        m.register("cwd", cwd);
        m
    }

    fn getenv(args: Vec<Value>) -> RuntimeResult<Value> {
        let key = require_string(&args, 0, "getenv")?;
        match env::var(&key) {
            Ok(val) => Ok(Value::Ok(Box::new(Value::Str(val)))),
            Err(_) => Ok(Value::Err(Box::new(Value::Str(format!(
                "environment variable '{}' not set",
                key
            ))))),
        }
    }

    fn setenv(args: Vec<Value>) -> RuntimeResult<Value> {
        let key = require_string(&args, 0, "setenv")?;
        let val = require_string(&args, 1, "setenv")?;
        unsafe {
            std::env::set_var(&key, &val);
        }
        Ok(Value::Void)
    }

    fn args(_args: Vec<Value>) -> RuntimeResult<Value> {
        let args: Vec<Value> = env::args().map(|a| Value::Str(a)).collect();
        Ok(Value::Array(args))
    }

    fn cwd(_args: Vec<Value>) -> RuntimeResult<Value> {
        match env::current_dir() {
            Ok(path) => Ok(Value::Ok(Box::new(Value::Str(
                path.to_string_lossy().to_string(),
            )))),
            Err(e) => Ok(Value::Err(Box::new(Value::Str(e.to_string())))),
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

// ------------------------------------------------------------------
// tx:time
// ------------------------------------------------------------------

pub mod time {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use typex_runtime::{days_to_ymd, ymd_to_days};

    pub fn module() -> StdModule {
        let mut m = StdModule::new();
        m.register("now", now);
        m.register("today", today);
        m.register("currentTime", current_time);
        m.register("dateTime", date_time);
        m.register("date", date);
        m.register("time", time);
        m.register("year", year);
        m.register("month", month);
        m.register("day", day);
        m.register("hour", hour);
        m.register("minute", minute);
        m.register("second", second);
        m.register("millisecond", millisecond);
        m.register("format", format);
        m.register("toDateTime", to_date_time);
        m.register("addDays", add_days);
        m.register("addMilliseconds", add_milliseconds);
        m.register("diffMilliseconds", diff_milliseconds);
        m.register("isBefore", is_before);
        m.register("isAfter", is_after);
        m
    }

    fn unix_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    // now() -> DateTime (current UTC datetime)
    fn now(_args: Vec<Value>) -> RuntimeResult<Value> {
        Ok(Value::DateTime(unix_ms()))
    }

    // today() -> Date (current UTC date)
    fn today(_args: Vec<Value>) -> RuntimeResult<Value> {
        let ms = unix_ms();
        let days = ms.div_euclid(86_400_000);
        Ok(Value::Date(days))
    }

    // currentTime() -> Time (current UTC time)
    fn current_time(_args: Vec<Value>) -> RuntimeResult<Value> {
        let ms = unix_ms();
        let time_ms = ms.rem_euclid(86_400_000) as u32;
        Ok(Value::Time(time_ms))
    }

    // dateTime(year, month, day, hour, minute, second, ms) -> DateTime
    fn date_time(args: Vec<Value>) -> RuntimeResult<Value> {
        let y = require_int(&args, 0, "dateTime")? as i32;
        let m = require_int(&args, 1, "dateTime")? as u32;
        let d = require_int(&args, 2, "dateTime")? as u32;
        let h = if args.len() > 3 {
            require_int(&args, 3, "dateTime")?
        } else {
            0
        };
        let min = if args.len() > 4 {
            require_int(&args, 4, "dateTime")?
        } else {
            0
        };
        let s = if args.len() > 5 {
            require_int(&args, 5, "dateTime")?
        } else {
            0
        };
        let ms = if args.len() > 6 {
            require_int(&args, 6, "dateTime")?
        } else {
            0
        };
        let days = ymd_to_days(y, m, d);
        let time_ms = h * 3_600_000 + min * 60_000 + s * 1_000 + ms;
        Ok(Value::DateTime(days * 86_400_000 + time_ms))
    }

    // date(year, month, day) -> Date
    fn date(args: Vec<Value>) -> RuntimeResult<Value> {
        let y = require_int(&args, 0, "date")? as i32;
        let m = require_int(&args, 1, "date")? as u32;
        let d = require_int(&args, 2, "date")? as u32;
        Ok(Value::Date(ymd_to_days(y, m, d)))
    }

    // time(hour, minute, second, ms?) -> Time
    fn time(args: Vec<Value>) -> RuntimeResult<Value> {
        let h = require_int(&args, 0, "time")?;
        let m = require_int(&args, 1, "time")?;
        let s = require_int(&args, 2, "time")?;
        let ms = if args.len() > 3 {
            require_int(&args, 3, "time")?
        } else {
            0
        };
        let total = (h * 3_600_000 + m * 60_000 + s * 1_000 + ms) as u32;
        Ok(Value::Time(total))
    }

    // year(Date | DateTime) -> int
    fn year(args: Vec<Value>) -> RuntimeResult<Value> {
        let days = extract_days(&args, 0, "year")?;
        let (y, _, _) = days_to_ymd(days);
        Ok(Value::Int(y as i64))
    }

    // month(Date | DateTime) -> int
    fn month(args: Vec<Value>) -> RuntimeResult<Value> {
        let days = extract_days(&args, 0, "month")?;
        let (_, m, _) = days_to_ymd(days);
        Ok(Value::Int(m as i64))
    }

    // day(Date | DateTime) -> int
    fn day(args: Vec<Value>) -> RuntimeResult<Value> {
        let days = extract_days(&args, 0, "day")?;
        let (_, _, d) = days_to_ymd(days);
        Ok(Value::Int(d as i64))
    }

    // hour(Time | DateTime) -> int
    fn hour(args: Vec<Value>) -> RuntimeResult<Value> {
        let ms = extract_time_ms(&args, 0, "hour")?;
        Ok(Value::Int((ms / 3_600_000) as i64))
    }

    // minute(Time | DateTime) -> int
    fn minute(args: Vec<Value>) -> RuntimeResult<Value> {
        let ms = extract_time_ms(&args, 0, "minute")?;
        Ok(Value::Int(((ms / 60_000) % 60) as i64))
    }

    // second(Time | DateTime) -> int
    fn second(args: Vec<Value>) -> RuntimeResult<Value> {
        let ms = extract_time_ms(&args, 0, "second")?;
        Ok(Value::Int(((ms / 1_000) % 60) as i64))
    }

    // millisecond(Time | DateTime) -> int
    fn millisecond(args: Vec<Value>) -> RuntimeResult<Value> {
        let ms = extract_time_ms(&args, 0, "millisecond")?;
        Ok(Value::Int((ms % 1_000) as i64))
    }

    // format(Date | Time | DateTime, pattern) -> string
    fn format(args: Vec<Value>) -> RuntimeResult<Value> {
        let pattern = require_string(&args, 1, "format")?;
        match args.first() {
            Some(Value::Date(days)) => {
                let (y, m, d) = days_to_ymd(*days);
                let s = pattern
                    .replace("YYYY", &format!("{:04}", y))
                    .replace("MM", &format!("{:02}", m))
                    .replace("DD", &format!("{:02}", d));
                Ok(Value::Str(s))
            }
            Some(Value::Time(ms)) => {
                let h = ms / 3_600_000;
                let min = (ms / 60_000) % 60;
                let s = (ms / 1_000) % 60;
                let millis = ms % 1_000;
                let result = pattern
                    .replace("HH", &format!("{:02}", h))
                    .replace("mm", &format!("{:02}", min))
                    .replace("ss", &format!("{:02}", s))
                    .replace("SSS", &format!("{:03}", millis));
                Ok(Value::Str(result))
            }
            Some(Value::DateTime(ms)) => {
                let days = ms.div_euclid(86_400_000);
                let time_ms = ms.rem_euclid(86_400_000) as u32;
                let (y, mo, d) = days_to_ymd(days);
                let h = time_ms / 3_600_000;
                let min = (time_ms / 60_000) % 60;
                let s = (time_ms / 1_000) % 60;
                let millis = time_ms % 1_000;
                let result = pattern
                    .replace("YYYY", &format!("{:04}", y))
                    .replace("MM", &format!("{:02}", mo))
                    .replace("DD", &format!("{:02}", d))
                    .replace("HH", &format!("{:02}", h))
                    .replace("mm", &format!("{:02}", min))
                    .replace("ss", &format!("{:02}", s))
                    .replace("SSS", &format!("{:03}", millis));
                Ok(Value::Str(result))
            }
            _ => Err(RuntimeError::new(
                "format: expected Date, Time, or DateTime",
            )),
        }
    }

    // toDateTime(Date) -> DateTime (midnight UTC)
    fn to_date_time(args: Vec<Value>) -> RuntimeResult<Value> {
        match args.first() {
            Some(Value::Date(days)) => Ok(Value::DateTime(days * 86_400_000)),
            Some(Value::DateTime(ms)) => Ok(Value::DateTime(*ms)),
            _ => Err(RuntimeError::new("toDateTime: expected Date or DateTime")),
        }
    }

    // addDays(Date | DateTime, days) -> same type
    fn add_days(args: Vec<Value>) -> RuntimeResult<Value> {
        let n = require_int(&args, 1, "addDays")?;
        match args.first() {
            Some(Value::Date(days)) => Ok(Value::Date(days + n)),
            Some(Value::DateTime(ms)) => Ok(Value::DateTime(ms + n * 86_400_000)),
            _ => Err(RuntimeError::new("addDays: expected Date or DateTime")),
        }
    }

    // addMilliseconds(DateTime | Time, ms) -> same type
    fn add_milliseconds(args: Vec<Value>) -> RuntimeResult<Value> {
        let n = require_int(&args, 1, "addMilliseconds")?;
        match args.first() {
            Some(Value::DateTime(ms)) => Ok(Value::DateTime(ms + n)),
            Some(Value::Time(ms)) => Ok(Value::Time((*ms as i64 + n) as u32)),
            _ => Err(RuntimeError::new(
                "addMilliseconds: expected DateTime or Time",
            )),
        }
    }

    // diffMilliseconds(DateTime, DateTime) -> int
    fn diff_milliseconds(args: Vec<Value>) -> RuntimeResult<Value> {
        match (args.first(), args.get(1)) {
            (Some(Value::DateTime(a)), Some(Value::DateTime(b))) => Ok(Value::Int(a - b)),
            _ => Err(RuntimeError::new(
                "diffMilliseconds: expected two DateTimes",
            )),
        }
    }

    // isBefore(a: DateTime, b: DateTime) -> boolean
    fn is_before(args: Vec<Value>) -> RuntimeResult<Value> {
        match (args.first(), args.get(1)) {
            (Some(Value::DateTime(a)), Some(Value::DateTime(b))) => Ok(Value::Bool(a < b)),
            (Some(Value::Date(a)), Some(Value::Date(b))) => Ok(Value::Bool(a < b)),
            (Some(Value::Time(a)), Some(Value::Time(b))) => Ok(Value::Bool(a < b)),
            _ => Err(RuntimeError::new("isBefore: type mismatch")),
        }
    }

    // isAfter(a: DateTime, b: DateTime) -> boolean
    fn is_after(args: Vec<Value>) -> RuntimeResult<Value> {
        match (args.first(), args.get(1)) {
            (Some(Value::DateTime(a)), Some(Value::DateTime(b))) => Ok(Value::Bool(a > b)),
            (Some(Value::Date(a)), Some(Value::Date(b))) => Ok(Value::Bool(a > b)),
            (Some(Value::Time(a)), Some(Value::Time(b))) => Ok(Value::Bool(a > b)),
            _ => Err(RuntimeError::new("isAfter: type mismatch")),
        }
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn extract_days(args: &[Value], idx: usize, fn_name: &str) -> RuntimeResult<i64> {
        match args.get(idx) {
            Some(Value::Date(d)) => Ok(*d),
            Some(Value::DateTime(ms)) => Ok(ms.div_euclid(86_400_000)),
            _ => Err(RuntimeError::new(format!(
                "{}: expected Date or DateTime",
                fn_name
            ))),
        }
    }

    fn extract_time_ms(args: &[Value], idx: usize, fn_name: &str) -> RuntimeResult<u32> {
        match args.get(idx) {
            Some(Value::Time(ms)) => Ok(*ms),
            Some(Value::DateTime(ms)) => Ok(ms.rem_euclid(86_400_000) as u32),
            _ => Err(RuntimeError::new(format!(
                "{}: expected Time or DateTime",
                fn_name
            ))),
        }
    }

    fn require_int(args: &[Value], idx: usize, fn_name: &str) -> RuntimeResult<i64> {
        match args.get(idx) {
            Some(Value::Int(n)) => Ok(*n),
            Some(Value::Uint(n)) => Ok(*n as i64),
            _ => Err(RuntimeError::new(format!(
                "{}: argument {} must be an integer",
                fn_name,
                idx + 1
            ))),
        }
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

    #[test]
    fn test_env_getenv_existing() {
        // PATH should always be set
        let result = call("tx:env", "getenv", vec![Value::Str("PATH".to_string())]).unwrap();
        assert!(matches!(result, Value::Ok(_)));
    }

    #[test]
    fn test_env_getenv_missing() {
        let result = call(
            "tx:env",
            "getenv",
            vec![Value::Str("TYPEX_TEST_NONEXISTENT_VAR".to_string())],
        )
        .unwrap();
        assert!(matches!(result, Value::Err(_)));
    }

    #[test]
    fn test_env_setenv_and_getenv() {
        let result = call(
            "tx:env",
            "setenv",
            vec![
                Value::Str("TYPEX_TEST_VAR".to_string()),
                Value::Str("hello_typex".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(result, Value::Void);

        let result = call(
            "tx:env",
            "getenv",
            vec![Value::Str("TYPEX_TEST_VAR".to_string())],
        )
        .unwrap();
        assert_eq!(
            result,
            Value::Ok(Box::new(Value::Str("hello_typex".to_string())))
        );
    }

    #[test]
    fn test_env_cwd() {
        let result = call("tx:env", "cwd", vec![]).unwrap();
        assert!(matches!(result, Value::Ok(_)));
    }

    #[test]
    fn test_env_args() {
        let result = call("tx:env", "args", vec![]).unwrap();
        assert!(matches!(result, Value::Array(_)));
    }

    #[test]
    fn test_registry_has_env() {
        let r = StdRegistry::new();
        assert!(r.has_module("tx:env"));
    }

    #[test]
    fn test_time_now() {
        let result = call("tx:time", "now", vec![]).unwrap();
        assert!(matches!(result, Value::DateTime(_)));
    }

    #[test]
    fn test_time_today() {
        let result = call("tx:time", "today", vec![]).unwrap();
        assert!(matches!(result, Value::Date(_)));
    }

    #[test]
    fn test_time_current_time() {
        let result = call("tx:time", "currentTime", vec![]).unwrap();
        assert!(matches!(result, Value::Time(_)));
    }

    #[test]
    fn test_time_date_construction() {
        let result = call(
            "tx:time",
            "date",
            vec![Value::Int(2024), Value::Int(1), Value::Int(15)],
        )
        .unwrap();
        assert!(matches!(result, Value::Date(_)));
        if let Value::Date(days) = result {
            assert_eq!(days, typex_runtime::ymd_to_days(2024, 1, 15));
        }
    }

    #[test]
    fn test_time_datetime_construction() {
        let result = call(
            "tx:time",
            "dateTime",
            vec![
                Value::Int(2024),
                Value::Int(6),
                Value::Int(15),
                Value::Int(12),
                Value::Int(30),
                Value::Int(0),
                Value::Int(0),
            ],
        )
        .unwrap();
        assert!(matches!(result, Value::DateTime(_)));
    }

    #[test]
    fn test_time_year_month_day() {
        let date = call(
            "tx:time",
            "date",
            vec![Value::Int(2024), Value::Int(3), Value::Int(21)],
        )
        .unwrap();

        let y = call("tx:time", "year", vec![date.clone()]).unwrap();
        let m = call("tx:time", "month", vec![date.clone()]).unwrap();
        let d = call("tx:time", "day", vec![date.clone()]).unwrap();

        assert_eq!(y, Value::Int(2024));
        assert_eq!(m, Value::Int(3));
        assert_eq!(d, Value::Int(21));
    }

    #[test]
    fn test_time_hour_minute_second() {
        let t = call(
            "tx:time",
            "time",
            vec![
                Value::Int(14),
                Value::Int(30),
                Value::Int(45),
                Value::Int(123),
            ],
        )
        .unwrap();

        let h = call("tx:time", "hour", vec![t.clone()]).unwrap();
        let min = call("tx:time", "minute", vec![t.clone()]).unwrap();
        let s = call("tx:time", "second", vec![t.clone()]).unwrap();
        let ms = call("tx:time", "millisecond", vec![t.clone()]).unwrap();

        assert_eq!(h, Value::Int(14));
        assert_eq!(min, Value::Int(30));
        assert_eq!(s, Value::Int(45));
        assert_eq!(ms, Value::Int(123));
    }

    #[test]
    fn test_time_format_date() {
        let date = call(
            "tx:time",
            "date",
            vec![Value::Int(2024), Value::Int(3), Value::Int(21)],
        )
        .unwrap();

        let result = call(
            "tx:time",
            "format",
            vec![date, Value::Str("YYYY-MM-DD".to_string())],
        )
        .unwrap();

        assert_eq!(result, Value::Str("2024-03-21".to_string()));
    }

    #[test]
    fn test_time_format_datetime() {
        let dt = call(
            "tx:time",
            "dateTime",
            vec![
                Value::Int(2024),
                Value::Int(6),
                Value::Int(15),
                Value::Int(14),
                Value::Int(30),
                Value::Int(0),
                Value::Int(0),
            ],
        )
        .unwrap();

        let result = call(
            "tx:time",
            "format",
            vec![dt, Value::Str("YYYY-MM-DD HH:mm:ss".to_string())],
        )
        .unwrap();

        assert_eq!(result, Value::Str("2024-06-15 14:30:00".to_string()));
    }

    #[test]
    fn test_time_add_days() {
        let date = call(
            "tx:time",
            "date",
            vec![Value::Int(2024), Value::Int(1), Value::Int(28)],
        )
        .unwrap();

        let result = call("tx:time", "addDays", vec![date, Value::Int(5)]).unwrap();

        let formatted = call(
            "tx:time",
            "format",
            vec![result, Value::Str("YYYY-MM-DD".to_string())],
        )
        .unwrap();

        assert_eq!(formatted, Value::Str("2024-02-02".to_string()));
    }

    #[test]
    fn test_time_is_before_after() {
        let a = call(
            "tx:time",
            "date",
            vec![Value::Int(2024), Value::Int(1), Value::Int(1)],
        )
        .unwrap();
        let b = call(
            "tx:time",
            "date",
            vec![Value::Int(2024), Value::Int(12), Value::Int(31)],
        )
        .unwrap();

        let before = call("tx:time", "isBefore", vec![a.clone(), b.clone()]).unwrap();
        let after = call("tx:time", "isAfter", vec![a.clone(), b.clone()]).unwrap();

        assert_eq!(before, Value::Bool(true));
        assert_eq!(after, Value::Bool(false));
    }

    #[test]
    fn test_registry_has_time() {
        let r = StdRegistry::new();
        assert!(r.has_module("tx:time"));
    }
}
