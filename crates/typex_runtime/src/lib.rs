use std::collections::HashMap;
use std::fmt;

// ------------------------------------------------------------------
// Value
// ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    // Primitives
    Int(i64),
    Uint(u64),
    Float(f64),
    Bool(bool),
    Char(char),
    Str(String),
    Null,

    // Compound
    Array(Vec<Value>),
    Record(HashMap<String, Value>),

    // Result
    Ok(Box<Value>),
    Err(Box<Value>),

    // Functions
    Fn(FnValue),

    // Void - no value
    Void,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnValue {
    pub name: String,
    pub params: Vec<String>,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Uint(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Char(c) => write!(f, "{}", c),
            Value::Str(s) => write!(f, "{}", s),
            Value::Null => write!(f, "null"),
            Value::Void => write!(f, ""),
            Value::Ok(v) => write!(f, "Ok({})", v),
            Value::Err(v) => write!(f, "Err({})", v),
            Value::Fn(fv) => write!(f, "<fn {}>", fv.name),
            Value::Array(elements) => {
                let items: Vec<String> = elements.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            Value::Record(fields) => {
                let items: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect();
                write!(f, "{{{}}}", items.join(", "))
            }
        }
    }
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Null => false,
            Value::Void => false,
            Value::Int(n) => *n != 0,
            Value::Uint(n) => *n != 0,
            _ => true,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Uint(_) => "uint",
            Value::Float(_) => "float",
            Value::Bool(_) => "boolean",
            Value::Char(_) => "char",
            Value::Str(_) => "string",
            Value::Null => "null",
            Value::Void => "void",
            Value::Ok(_) => "Ok",
            Value::Err(_) => "Err",
            Value::Fn(_) => "fn",
            Value::Array(_) => "Array",
            Value::Record(_) => "Record",
        }
    }
}

// ------------------------------------------------------------------
// Format strings
// ------------------------------------------------------------------

/// Format a TypeX format string with positional {} or named {ident} args
pub fn format_string(template: &str, args: &[Value]) -> String {
    let mut result = String::new();
    let mut chars = template.chars().peekable();
    let mut arg_index = 0;

    while let Some(ch) = chars.next() {
        if ch == '{' {
            // peek ahead
            let mut inner = String::new();
            let mut closed = false;
            for c in chars.by_ref() {
                if c == '}' {
                    closed = true;
                    break;
                }
                inner.push(c);
            }
            if !closed {
                result.push('{');
                result.push_str(&inner);
                continue;
            }
            if inner.is_empty() {
                // positional {}
                if let Some(val) = args.get(arg_index) {
                    result.push_str(&val.to_string());
                    arg_index += 1;
                }
            } else if let Ok(idx) = inner.parse::<usize>() {
                // indexed {42}
                if let Some(val) = args.get(idx) {
                    result.push_str(&val.to_string());
                }
            } else {
                // named {ident} - look up by name in args
                // for now just output the name as-is since we don't have
                // named arg lookup at runtime yet
                result.push('{');
                result.push_str(&inner);
                result.push('}');
            }
        } else {
            result.push(ch);
        }
    }
    result
}

// ------------------------------------------------------------------
// Runtime errors
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum RuntimeErrorKind {
    Return(Value),
    Panic(String),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub kind: RuntimeErrorKind,
    pub message: String,
}

impl RuntimeError {
    pub fn new(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self {
            kind: RuntimeErrorKind::Error(msg.clone()),
            message: msg,
        }
    }
    pub fn new_return(val: Value) -> Self {
        Self {
            kind: RuntimeErrorKind::Return(val),
            message: "__return__".to_string(),
        }
    }
    pub fn new_panic(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self {
            kind: RuntimeErrorKind::Panic(msg.clone()),
            message: msg,
        }
    }
    pub fn is_return(&self) -> bool {
        matches!(self.kind, RuntimeErrorKind::Return(_))
    }
    pub fn is_panic(&self) -> bool {
        matches!(self.kind, RuntimeErrorKind::Panic(_))
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            RuntimeErrorKind::Return(_) => write!(f, "return"),
            RuntimeErrorKind::Panic(msg) => write!(f, "panic: {}", msg),
            RuntimeErrorKind::Error(msg) => write!(f, "runtime error: {}", msg),
        }
    }
}

pub type RuntimeResult<T> = Result<T, RuntimeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_display() {
        assert_eq!(Value::Int(42).to_string(), "42");
        assert_eq!(Value::Uint(100).to_string(), "100");
        assert_eq!(Value::Float(3.14).to_string(), "3.14");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Char('x').to_string(), "x");
        assert_eq!(Value::Str("hello".to_string()).to_string(), "hello");
        assert_eq!(Value::Null.to_string(), "null");
        assert_eq!(Value::Void.to_string(), "");
    }

    #[test]
    fn test_value_is_truthy() {
        assert!(Value::Bool(true).is_truthy());
        assert!(!Value::Bool(false).is_truthy());
        assert!(!Value::Null.is_truthy());
        assert!(Value::Int(1).is_truthy());
        assert!(!Value::Int(0).is_truthy());
        assert!(Value::Str("hello".to_string()).is_truthy());
    }

    #[test]
    fn test_ok_err_display() {
        assert_eq!(Value::Ok(Box::new(Value::Int(42))).to_string(), "Ok(42)");
        assert_eq!(
            Value::Err(Box::new(Value::Str("oops".to_string()))).to_string(),
            "Err(oops)"
        );
    }

    #[test]
    fn test_array_display() {
        let arr = Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(arr.to_string(), "[1, 2, 3]");
    }

    #[test]
    fn test_format_string_positional() {
        let args = vec![
            Value::Int(1),
            Value::Str("two".to_string()),
            Value::Float(3.14),
        ];
        let result = format_string("hi {} {} {}", &args);
        assert_eq!(result, "hi 1 two 3.14");
    }

    #[test]
    fn test_format_string_indexed() {
        let args = vec![Value::Int(1), Value::Int(2), Value::Int(3)];
        let result = format_string("{2} {0} {1}", &args);
        assert_eq!(result, "3 1 2");
    }

    #[test]
    fn test_format_string_empty() {
        let result = format_string("no placeholders", &[]);
        assert_eq!(result, "no placeholders");
    }

    #[test]
    fn test_type_name() {
        assert_eq!(Value::Int(0).type_name(), "int");
        assert_eq!(Value::Bool(true).type_name(), "boolean");
        assert_eq!(Value::Str("".to_string()).type_name(), "string");
        assert_eq!(Value::Null.type_name(), "null");
        assert_eq!(Value::Array(vec![]).type_name(), "Array");
    }
}
