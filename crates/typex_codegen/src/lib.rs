use cranelift_codegen::Context;
use cranelift_codegen::entity::EntityRef;
use cranelift_codegen::ir::StackSlotData;
use cranelift_codegen::ir::StackSlotKind;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::*;
use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, MemFlags, UserFuncName};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{DataDescription, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use std::collections::HashMap;
use target_lexicon::Triple;
use typex_ast::*;

// ------------------------------------------------------------------
// Errors
// ------------------------------------------------------------------

#[derive(Debug)]
pub struct CodegenError(pub String);

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "codegen error: {}", self.0)
    }
}

impl From<cranelift_module::ModuleError> for CodegenError {
    fn from(e: cranelift_module::ModuleError) -> Self {
        CodegenError(e.to_string())
    }
}

pub type CodegenResult<T> = Result<T, CodegenError>;

// ------------------------------------------------------------------
// Codegen types
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
enum CgTy {
    Int,   // i64
    Float, // f64
    Bool,  // i8
    Void,
}

impl CgTy {
    fn cranelift_type(&self) -> Option<cranelift_codegen::ir::Type> {
        match self {
            CgTy::Int => Some(I64),
            CgTy::Float => Some(F64),
            CgTy::Bool => Some(I8),
            CgTy::Void => None,
        }
    }
}

fn type_from_ast(ty: &TypeExpr) -> CgTy {
    match ty {
        TypeExpr::Named(n) => match n.name.as_str() {
            "int" | "int64" | "int32" | "int16" | "int8" => CgTy::Int,
            "uint" | "uint64" | "uint32" | "uint16" | "uint8" => CgTy::Int,
            "float" | "float64" | "float32" => CgTy::Float,
            "boolean" => CgTy::Bool,
            _ => CgTy::Void,
        },
        _ => CgTy::Void,
    }
}

// ------------------------------------------------------------------
// Compiler
// ------------------------------------------------------------------

pub struct Compiler {
    module: ObjectModule,
    // map of string literals to data ids
    string_data: HashMap<String, cranelift_module::DataId>,
}

impl Compiler {
    pub fn new(_target: Option<Triple>) -> CodegenResult<Self> {
        let mut settings = settings::builder();
        settings.set("opt_level", "none").unwrap();
        settings.set("is_pic", "true").unwrap();

        let flags = settings::Flags::new(settings);
        let isa = cranelift_native::builder_with_options(true)
            .map_err(|e| CodegenError(e.to_string()))?
            .finish(flags)
            .map_err(|e| CodegenError(e.to_string()))?;

        let obj_builder = ObjectBuilder::new(
            isa,
            "typex_output",
            cranelift_module::default_libcall_names(),
        )?;

        Ok(Self {
            module: ObjectModule::new(obj_builder),
            string_data: HashMap::new(),
        })
    }

    // ------------------------------------------------------------------
    // Module compilation
    // ------------------------------------------------------------------

    pub fn compile_module(&mut self, module: &typex_ast::Module) -> CodegenResult<Vec<u8>> {
        // declare puts from libc for println
        // self.declare_puts()?;

        // first pass: declare all functions
        let mut fn_ids = HashMap::new();
        for item in &module.items {
            if let Item::Function(f) = item {
                let id = self.declare_function(f)?;
                fn_ids.insert(f.name.name.clone(), id);
            }
        }

        // second pass: compile function bodies
        for item in &module.items {
            if let Item::Function(f) = item {
                let id = fn_ids[&f.name.name];
                self.compile_function(f, id, &fn_ids)?;
            }
        }

        // take ownership of module to finish it
        let mut settings = settings::builder();
        settings.set("opt_level", "speed").unwrap();
        settings.set("is_pic", "false").unwrap();
        let flags = settings::Flags::new(settings);
        let isa = cranelift_native::builder_with_options(true)
            .map_err(|e| CodegenError(e.to_string()))?
            .finish(flags)
            .map_err(|e| CodegenError(e.to_string()))?;
        let obj_builder = ObjectBuilder::new(
            isa,
            "typex_output",
            cranelift_module::default_libcall_names(),
        )
        .map_err(|e| CodegenError(e.to_string()))?;
        let dummy = ObjectModule::new(obj_builder);
        let real_module = std::mem::replace(&mut self.module, dummy);

        let obj = real_module.finish();
        let bytes = obj.emit().map_err(|e| CodegenError(e.to_string()))?;

        wrap_executable(bytes)
    }

    fn declare_function(&mut self, f: &FunctionDef) -> CodegenResult<cranelift_module::FuncId> {
        let mut sig = self.module.make_signature();

        for param in &f.params {
            let ty = type_from_ast(&param.ty);
            if let Some(cl_ty) = ty.cranelift_type() {
                sig.params.push(AbiParam::new(cl_ty));
            }
        }

        if let Some(ret_ty) = &f.return_type {
            let ty = type_from_ast(ret_ty);
            if let Some(cl_ty) = ty.cranelift_type() {
                sig.returns.push(AbiParam::new(cl_ty));
            }
        }

        // main is exported
        let linkage = if f.name.name == "main" {
            Linkage::Export
        } else {
            Linkage::Local
        };

        let id = self.module.declare_function(&f.name.name, linkage, &sig)?;
        Ok(id)
    }

    fn compile_function(
        &mut self,
        f: &FunctionDef,
        func_id: cranelift_module::FuncId,
        fn_ids: &HashMap<String, cranelift_module::FuncId>,
    ) -> CodegenResult<()> {
        let mut sig = self.module.make_signature();
        for param in &f.params {
            let ty = type_from_ast(&param.ty);
            if let Some(cl_ty) = ty.cranelift_type() {
                sig.params.push(AbiParam::new(cl_ty));
            }
        }
        if let Some(ret_ty) = &f.return_type {
            let ty = type_from_ast(ret_ty);
            if let Some(cl_ty) = ty.cranelift_type() {
                sig.returns.push(AbiParam::new(cl_ty));
            }
        }

        let mut cl_fn = Function::with_name_signature(UserFuncName::user(0, func_id.as_u32()), sig);

        let mut fn_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut cl_fn, &mut fn_ctx);

        // entry block
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        // define param variables
        let mut vars: HashMap<String, Variable> = HashMap::new();
        let mut var_idx = 0;

        for (i, param) in f.params.iter().enumerate() {
            let ty = type_from_ast(&param.ty);
            if let Some(cl_ty) = ty.cranelift_type() {
                let var = Variable::new(var_idx);
                var_idx += 1;
                builder.declare_var(var, cl_ty);
                let val = builder.block_params(entry)[i];
                builder.def_var(var, val);
                vars.insert(param.name.name.clone(), var);
            }
        }

        let ret_ty = f
            .return_type
            .as_ref()
            .map(|t| type_from_ast(t))
            .unwrap_or(CgTy::Void);

        let mut fcx = FnCompiler {
            builder,
            vars,
            var_idx,
            module: &mut self.module,
            fn_ids,
            string_data: &mut self.string_data,
            terminated: false,
            arrays: HashMap::new(),
        };

        fcx.compile_block(&f.body)?;

        // implicit return 0 for main if no explicit return
        if !fcx.terminated {
            match ret_ty {
                CgTy::Int | CgTy::Bool => {
                    let zero = fcx.builder.ins().iconst(I64, 0);
                    fcx.builder.ins().return_(&[zero]);
                }
                CgTy::Float => {
                    let zero = fcx.builder.ins().f64const(0.0);
                    fcx.builder.ins().return_(&[zero]);
                }
                CgTy::Void => {
                    fcx.builder.ins().return_(&[]);
                }
            }
        }

        fcx.builder.finalize();

        let mut ctx = Context::for_function(cl_fn);

        // verify before defining
        if let Err(errors) = verify_function(&ctx.func, &*self.module.isa()) {
            return Err(CodegenError(format!(
                "verifier errors in '{}': {}",
                f.name.name, errors
            )));
        }
        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|e| CodegenError(format!("define_function '{}': {}", f.name.name, e)))?;

        Ok(())
    }
}

// ------------------------------------------------------------------
// Function compiler
// ------------------------------------------------------------------

struct FnCompiler<'a> {
    builder: FunctionBuilder<'a>,
    vars: HashMap<String, Variable>,
    var_idx: usize,
    module: &'a mut ObjectModule,
    fn_ids: &'a HashMap<String, cranelift_module::FuncId>,
    string_data: &'a mut HashMap<String, cranelift_module::DataId>,
    terminated: bool,
    arrays: HashMap<String, (cranelift_codegen::ir::StackSlot, usize)>, // name -> (slot, length)
}

impl<'a> FnCompiler<'a> {
    fn new_var(&mut self, ty: cranelift_codegen::ir::Type) -> Variable {
        let var = Variable::new(self.var_idx);
        self.var_idx += 1;
        self.builder.declare_var(var, ty);
        var
    }

    fn define_string(&mut self, s: &str) -> CodegenResult<cranelift_module::DataId> {
        if let Some(id) = self.string_data.get(s) {
            return Ok(*id);
        }
        let mut data = DataDescription::new();
        let mut bytes = s.as_bytes().to_vec();
        bytes.push(0);
        data.define(bytes.into_boxed_slice());
        let id = self.module.declare_anonymous_data(false, false)?;
        self.module.define_data(id, &data)?;
        self.string_data.insert(s.to_string(), id);
        Ok(id)
    }

    // ------------------------------------------------------------------
    // Blocks & statements
    // ------------------------------------------------------------------

    fn compile_block(&mut self, block: &Block) -> CodegenResult<()> {
        for stmt in &block.stmts {
            if self.terminated {
                break;
            }
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> CodegenResult<()> {
        match stmt {
            Stmt::Return(expr, _) => {
                match expr {
                    Some(e) => {
                        let val = self.compile_expr(e)?;
                        self.builder.ins().return_(&[val]);
                    }
                    None => {
                        self.builder.ins().return_(&[]);
                    }
                }
                self.terminated = true;
            }
            Stmt::Let(l) => {
                if let Some(Expr::Array(elements, _)) = &l.value {
                    self.compile_array_let(&l.name.name, elements)?;
                } else {
                    let cl_ty =
                        l.ty.as_ref()
                            .map(|t| type_from_ast(t))
                            .and_then(|t| t.cranelift_type())
                            .unwrap_or(I64);
                    let var = self.new_var(cl_ty);
                    if let Some(expr) = &l.value {
                        let val = self.compile_expr(expr)?;
                        self.builder.def_var(var, val);
                    } else {
                        let zero = self.builder.ins().iconst(cl_ty, 0);
                        self.builder.def_var(var, zero);
                    }
                    self.vars.insert(l.name.name.clone(), var);
                }
            }
            Stmt::Const(c) => {
                if let Expr::Array(elements, _) = &c.value {
                    self.compile_array_let(&c.name.name, elements)?;
                } else {
                    let cl_ty =
                        c.ty.as_ref()
                            .map(|t| type_from_ast(t))
                            .and_then(|t| t.cranelift_type())
                            .unwrap_or(I64);
                    let var = self.new_var(cl_ty);
                    let val = self.compile_expr(&c.value)?;
                    self.builder.def_var(var, val);
                    self.vars.insert(c.name.name.clone(), var);
                }
            }
            Stmt::If(i) => self.compile_if(i)?,
            Stmt::Expr(e) => {
                self.compile_expr(e)?;
            }
            Stmt::Switch(s) => self.compile_switch(s)?,
            Stmt::For(f) => self.compile_for(f)?,
            _ => {}
        }
        Ok(())
    }

    fn compile_if(&mut self, i: &IfStmt) -> CodegenResult<()> {
        let cond_val = self.compile_expr(&i.condition)?;

        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();

        self.builder
            .ins()
            .brif(cond_val, then_block, &[], else_block, &[]);

        // then
        self.builder.switch_to_block(then_block);
        self.builder.seal_block(then_block);
        self.compile_block(&i.then_block)?;
        let then_terminated = self.terminated;
        if !self.terminated {
            self.builder.ins().jump(merge_block, &[]);
        }
        self.terminated = false;

        // else
        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        if let Some(else_blk) = &i.else_block {
            self.compile_block(else_blk)?;
        }
        let else_terminated = self.terminated;
        if !self.terminated {
            self.builder.ins().jump(merge_block, &[]);
        }
        self.terminated = false;

        // merge - only switch to it if at least one branch jumps to it
        if !then_terminated || !else_terminated {
            self.builder.switch_to_block(merge_block);
            self.builder.seal_block(merge_block);
        } else {
            // both branches terminated (e.g. both returned)
            // merge block is unreachable - seal it anyway to keep Cranelift happy
            self.builder.switch_to_block(merge_block);
            self.builder.seal_block(merge_block);
            // mark as terminated since all paths returned
            self.terminated = true;
        }

        Ok(())
    }

    fn compile_switch(&mut self, s: &SwitchStmt) -> CodegenResult<()> {
        let switch_val = self.compile_expr(&s.value)?;
        let merge_block = self.builder.create_block();

        for case in &s.cases {
            let case_val = self.compile_expr(&case.value)?;
            let cmp = self.builder.ins().icmp(IntCC::Equal, switch_val, case_val);

            let case_body_block = self.builder.create_block();
            let next_block = self.builder.create_block();

            self.builder
                .ins()
                .brif(cmp, case_body_block, &[], next_block, &[]);

            // case body
            self.builder.switch_to_block(case_body_block);
            self.builder.seal_block(case_body_block);
            self.compile_block(&case.body)?;
            if !self.terminated {
                self.builder.ins().jump(merge_block, &[]);
            }
            self.terminated = false;

            // continue to next check
            self.builder.switch_to_block(next_block);
            self.builder.seal_block(next_block);
        }

        // default case (or fallthrough if none)
        if let Some(default) = &s.default {
            self.compile_block(default)?;
        }
        if !self.terminated {
            self.builder.ins().jump(merge_block, &[]);
        }
        self.terminated = false;

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        Ok(())
    }

    fn compile_array_let(&mut self, name: &str, elements: &[Expr]) -> CodegenResult<()> {
        let len = elements.len();
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            (len * 8) as u32,
            3, // align to 8 bytes (2^3)
        ));

        for (i, elem) in elements.iter().enumerate() {
            let val = self.compile_expr(elem)?;
            let offset = (i * 8) as i32;
            self.builder.ins().stack_store(val, slot, offset);
        }

        self.arrays.insert(name.to_string(), (slot, len));
        Ok(())
    }

    fn compile_for(&mut self, f: &ForStmt) -> CodegenResult<()> {
        match f {
            ForStmt::Array {
                index,
                value,
                iter,
                body,
                ..
            } => {
                // only support iterating a named array variable for now
                let array_name = match iter.as_ref() {
                    Expr::Ident(i) => i.name.clone(),
                    _ => {
                        return Err(CodegenError(
                            "for loop codegen only supports iterating a named array variable"
                                .to_string(),
                        ));
                    }
                };

                let (slot, len) = *self
                    .arrays
                    .get(&array_name)
                    .ok_or_else(|| CodegenError(format!("unknown array '{}'", array_name)))?;

                // index variable (i64 counter)
                let idx_var = self.new_var(I64);
                let zero = self.builder.ins().iconst(I64, 0);
                self.builder.def_var(idx_var, zero);

                let header_block = self.builder.create_block();
                let body_block = self.builder.create_block();
                let exit_block = self.builder.create_block();

                self.builder.ins().jump(header_block, &[]);

                // header: check idx < len
                self.builder.switch_to_block(header_block);
                let idx_val = self.builder.use_var(idx_var);
                let len_val = self.builder.ins().iconst(I64, len as i64);
                let cmp = self
                    .builder
                    .ins()
                    .icmp(IntCC::SignedLessThan, idx_val, len_val);
                self.builder
                    .ins()
                    .brif(cmp, body_block, &[], exit_block, &[]);

                // body
                self.builder.switch_to_block(body_block);
                self.builder.seal_block(body_block);

                // bind index/value variables for this iteration
                if let Some(idx_ident) = index {
                    let bound_idx = self.new_var(I64);
                    let cur_idx = self.builder.use_var(idx_var);
                    self.builder.def_var(bound_idx, cur_idx);
                    self.vars.insert(idx_ident.name.clone(), bound_idx);
                }
                if let Some(val_ident) = value {
                    let cur_idx = self.builder.use_var(idx_var);
                    let offset = self.builder.ins().imul_imm(cur_idx, 8);
                    let base_addr = self.builder.ins().stack_addr(I64, slot, 0);
                    let addr = self.builder.ins().iadd(base_addr, offset);
                    let elem_val = self.builder.ins().load(I64, MemFlags::new(), addr, 0);
                    let bound_val = self.new_var(I64);
                    self.builder.def_var(bound_val, elem_val);
                    self.vars.insert(val_ident.name.clone(), bound_val);
                }

                self.compile_block(body)?;

                if !self.terminated {
                    let cur_idx = self.builder.use_var(idx_var);
                    let one = self.builder.ins().iconst(I64, 1);
                    let next_idx = self.builder.ins().iadd(cur_idx, one);
                    self.builder.def_var(idx_var, next_idx);
                    self.builder.ins().jump(header_block, &[]);
                }
                self.terminated = false;

                self.builder.seal_block(header_block);
                self.builder.switch_to_block(exit_block);
                self.builder.seal_block(exit_block);

                Ok(())
            }
            ForStmt::Object { .. } => Err(CodegenError(
                "for...of object loops not yet supported in codegen".to_string(),
            )),
            ForStmt::Str { .. } => Err(CodegenError(
                "for...in string loops not yet supported in codegen".to_string(),
            )),
        }
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    fn compile_expr(&mut self, expr: &Expr) -> CodegenResult<cranelift_codegen::ir::Value> {
        match expr {
            Expr::Lit(lit) => self.compile_lit(lit),
            Expr::Ident(i) => self.compile_ident(i),
            Expr::BinOp(l, op, r, _) => self.compile_binop(l, op, r),
            Expr::UnaryOp(op, e, _) => self.compile_unary(op, e),
            Expr::Call(f, args, _) => self.compile_call(f, args),
            Expr::Ternary(c, t, e, _) => self.compile_ternary(c, t, e),
            Expr::Assign(lhs, rhs, _) => {
                let val = self.compile_expr(rhs)?;
                match lhs.as_ref() {
                    Expr::Ident(i) => {
                        if let Some(var) = self.vars.get(&i.name).copied() {
                            self.builder.def_var(var, val);
                            Ok(val)
                        } else {
                            Err(CodegenError(format!("undefined variable '{}'", i.name)))
                        }
                    }
                    _ => Err(CodegenError(
                        "invalid assignment target in codegen".to_string(),
                    )),
                }
            }
            _ => Err(CodegenError(format!(
                "unsupported expression in codegen: {:?}",
                std::mem::discriminant(expr)
            ))),
        }
    }

    fn compile_lit(&mut self, lit: &Lit) -> CodegenResult<cranelift_codegen::ir::Value> {
        Ok(match lit {
            Lit::Int(n, _) => self.builder.ins().iconst(I64, *n),
            Lit::Float(f, _) => self.builder.ins().f64const(*f),
            Lit::Bool(b, _) => self.builder.ins().iconst(I8, if *b { 1 } else { 0 }),
            Lit::Null(_) => self.builder.ins().iconst(I64, 0),
            Lit::Str(s, _) => {
                let data_id = self.define_string(s)?;
                let gv = self.module.declare_data_in_func(data_id, self.builder.func);
                self.builder.ins().symbol_value(I64, gv)
            }
            Lit::Char(c, _) => self.builder.ins().iconst(I32, *c as i64),
        })
    }

    fn compile_ident(&mut self, i: &Ident) -> CodegenResult<cranelift_codegen::ir::Value> {
        if let Some(var) = self.vars.get(&i.name) {
            Ok(self.builder.use_var(*var))
        } else {
            Err(CodegenError(format!("undefined variable '{}'", i.name)))
        }
    }

    fn compile_binop(
        &mut self,
        l: &Expr,
        op: &BinOp,
        r: &Expr,
    ) -> CodegenResult<cranelift_codegen::ir::Value> {
        let lv = self.compile_expr(l)?;
        let rv = self.compile_expr(r)?;

        Ok(match op {
            BinOp::Add => self.builder.ins().iadd(lv, rv),
            BinOp::Sub => self.builder.ins().isub(lv, rv),
            BinOp::Mul => self.builder.ins().imul(lv, rv),
            BinOp::Div => self.builder.ins().sdiv(lv, rv),
            BinOp::Mod => self.builder.ins().srem(lv, rv),
            BinOp::Eq => {
                let res = self.builder.ins().icmp(IntCC::Equal, lv, rv);
                self.builder.ins().uextend(I64, res)
            }
            BinOp::NotEq => {
                let res = self.builder.ins().icmp(IntCC::NotEqual, lv, rv);
                self.builder.ins().uextend(I64, res)
            }
            BinOp::Lt => {
                let res = self.builder.ins().icmp(IntCC::SignedLessThan, lv, rv);
                self.builder.ins().uextend(I64, res)
            }
            BinOp::Lte => {
                let res = self
                    .builder
                    .ins()
                    .icmp(IntCC::SignedLessThanOrEqual, lv, rv);
                self.builder.ins().uextend(I64, res)
            }
            BinOp::Gt => {
                let res = self.builder.ins().icmp(IntCC::SignedGreaterThan, lv, rv);
                self.builder.ins().uextend(I64, res)
            }
            BinOp::Gte => {
                let res = self
                    .builder
                    .ins()
                    .icmp(IntCC::SignedGreaterThanOrEqual, lv, rv);
                self.builder.ins().uextend(I64, res)
            }
            BinOp::And => self.builder.ins().band(lv, rv),
            BinOp::Or => self.builder.ins().bor(lv, rv),
        })
    }

    fn compile_unary(
        &mut self,
        op: &UnaryOp,
        e: &Expr,
    ) -> CodegenResult<cranelift_codegen::ir::Value> {
        let val = self.compile_expr(e)?;
        Ok(match op {
            UnaryOp::Neg => self.builder.ins().ineg(val),
            UnaryOp::Not => {
                let one = self.builder.ins().iconst(I8, 1);
                self.builder.ins().bxor(val, one)
            }
        })
    }

    fn compile_call(
        &mut self,
        f: &Expr,
        args: &[Expr],
    ) -> CodegenResult<cranelift_codegen::ir::Value> {
        let arg_vals: Vec<cranelift_codegen::ir::Value> = args
            .iter()
            .map(|a| self.compile_expr(a))
            .collect::<CodegenResult<Vec<_>>>()?;

        match f {
            Expr::Ident(i) => {
                // println - call puts
                if i.name == "println" || i.name == "print" {
                    if let Some(Expr::Lit(Lit::Str(s, _))) = args.first() {
                        // for now: if no args, use puts; if one int arg, use printf with explicit cast
                        if args.len() == 1 {
                            // no substitution args - just puts
                            let fmt = if i.name == "println" {
                                format!("{}\n", s)
                            } else {
                                s.clone()
                            };
                            let data_id = self.define_string(&fmt)?;
                            let gv = self.module.declare_data_in_func(data_id, self.builder.func);
                            let ptr = self.builder.ins().symbol_value(I64, gv);

                            let puts_id = self
                                .module
                                .declare_function("puts", Linkage::Import, &{
                                    let mut sig = self.module.make_signature();
                                    sig.params.push(AbiParam::new(I64));
                                    sig.returns.push(AbiParam::new(I32));
                                    sig
                                })
                                .map_err(|e| CodegenError(e.to_string()))?;

                            let puts_ref =
                                self.module.declare_func_in_func(puts_id, self.builder.func);
                            let _call = self.builder.ins().call(puts_ref, &[ptr]);
                            return Ok(self.builder.ins().iconst(I64, 0));
                        } else {
                            // has args - use puts for the string part + itoa approach
                            // compile the single int arg
                            let arg_val = self.compile_expr(&args[1])?;

                            let fmt = if i.name == "println" {
                                format!("{}\n", s.replace("{}", "%lld"))
                            } else {
                                s.replace("{}", "%lld")
                            };

                            let data_id = self.define_string(&fmt)?;
                            let gv = self.module.declare_data_in_func(data_id, self.builder.func);
                            let ptr = self.builder.ins().symbol_value(I64, gv);

                            // declare printf with exact signature - no variadic
                            let mut printf_sig = self.module.make_signature();
                            printf_sig.call_conv = CallConv::AppleAarch64;
                            printf_sig.params.push(AbiParam::new(I64));
                            printf_sig.params.push(AbiParam::new(I64));
                            printf_sig.returns.push(AbiParam::new(I32));

                            let printf_id = self
                                .module
                                .declare_function("tx_print_int", Linkage::Import, &printf_sig)
                                .map_err(|e| CodegenError(e.to_string()))?;

                            let printf_ref = self
                                .module
                                .declare_func_in_func(printf_id, self.builder.func);

                            let _call = self.builder.ins().call(printf_ref, &[ptr, arg_val]);
                            return Ok(self.builder.ins().iconst(I64, 0));
                        }
                    }
                }

                // user function call
                if let Some(&fn_id) = self.fn_ids.get(&i.name) {
                    let fn_ref = self.module.declare_func_in_func(fn_id, self.builder.func);
                    let call = self.builder.ins().call(fn_ref, &arg_vals);
                    let results = self.builder.inst_results(call);
                    if results.is_empty() {
                        return Ok(self.builder.ins().iconst(I64, 0));
                    }
                    return Ok(results[0]);
                }

                Err(CodegenError(format!("unknown function '{}'", i.name)))
            }
            _ => Err(CodegenError(
                "complex call expressions not yet supported".to_string(),
            )),
        }
    }

    fn compile_ternary(
        &mut self,
        cond: &Expr,
        then: &Expr,
        else_: &Expr,
    ) -> CodegenResult<cranelift_codegen::ir::Value> {
        let cond_val = self.compile_expr(cond)?;

        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();

        // result variable
        let result_var = self.new_var(I64);

        self.builder
            .ins()
            .brif(cond_val, then_block, &[], else_block, &[]);

        // then
        self.builder.switch_to_block(then_block);
        self.builder.seal_block(then_block);
        let then_val = self.compile_expr(then)?;
        self.builder.def_var(result_var, then_val);
        self.builder.ins().jump(merge_block, &[]);

        // else
        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        let else_val = self.compile_expr(else_)?;
        self.builder.def_var(result_var, else_val);
        self.builder.ins().jump(merge_block, &[]);

        // merge
        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        Ok(self.builder.use_var(result_var))
    }
}

// ------------------------------------------------------------------
// Executable wrapping
// ------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn wrap_executable(obj_bytes: Vec<u8>) -> CodegenResult<Vec<u8>> {
    // On macOS we write the object file and use the system linker
    // Direct Mach-O generation is complex - use ld for now
    Ok(obj_bytes)
}

#[cfg(target_os = "linux")]
fn wrap_executable(obj_bytes: Vec<u8>) -> CodegenResult<Vec<u8>> {
    Ok(obj_bytes)
}

#[cfg(target_os = "windows")]
fn wrap_executable(obj_bytes: Vec<u8>) -> CodegenResult<Vec<u8>> {
    Ok(obj_bytes)
}

// ------------------------------------------------------------------
// Public API
// ------------------------------------------------------------------

pub fn compile(module: &typex_ast::Module, target: Option<Triple>) -> CodegenResult<Vec<u8>> {
    let mut compiler = Compiler::new(target)?;
    compiler.compile_module(module)
}
