use ast::{self, SourceSlice, Stmt};
use astvisitor::Visitor;
use color::LinearRGBA;
use std::collections::{HashMap, HashSet};
use std::error;
use std::error::Error;
use std::fmt;
use types::{BinaryOperator, BlendMode, RenderTargetFormat, ZTestMode, CullingMode};

#[derive(Debug, Clone)]
pub struct SemanticError {
    slice: SourceSlice,
    error: String,
}
pub struct SourceSnippet<'a> {
    source: &'a str,
    slice: SourceSlice,
}
impl SemanticError {
    pub fn error_from_ast(ast: &dyn ast::AstNode, error: String) -> SemanticError {
        SemanticError {
            slice: ast.source_slice(),
            error: error,
        }
    }

    pub fn source_snippet<'a>(&self, source: &'a str) -> SourceSnippet<'a> {
        SourceSnippet {
            source: source,
            slice: self.slice,
        }
    }
}
impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.description(), self.error)
    }
}
impl<'a> SourceSnippet<'a> {
    pub fn new<'n>(slice: SourceSlice, source: &'n str) -> SourceSnippet<'n> {
        SourceSnippet {
            slice: slice,
            source: source,
        }
    }

    fn transform_position(&self, pos: usize) -> Option<(usize, usize)> {
        let mut counter = 0;
        for (l_idx, l) in self.source.split('\n').enumerate() {
            if counter + l.len() + 1 > pos {
                return Some((l_idx, pos - counter));
            }
            counter += l.len() + 1;
        }
        None
    }
}
impl<'a> fmt::Display for SourceSnippet<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let lo_pos = self.transform_position(self.slice.begin).unwrap();
        let hi_pos = self.transform_position(self.slice.end).unwrap();

        let error_highlighting: String;
        if lo_pos == hi_pos {
            let source_line = self.source.lines().skip(lo_pos.0).next().unwrap();
            error_highlighting = format!("{}\n{}^", source_line, " ".repeat(lo_pos.1));
        } else {
            let mut source_lines = String::new();
            let mut caret = lo_pos.1;
            for line in lo_pos.0..hi_pos.0 + 1 {
                let source_line = self.source.lines().skip(line).next().unwrap();
                let underline = " ".repeat(caret + 5)
                    + &"~".repeat(if line != hi_pos.0 {
                        source_line.len() - caret
                    } else {
                        hi_pos.1 - caret
                    });
                caret = 0;

                source_lines += &format!("{:03}: {}\n{}\n", line + 1, source_line, &underline);
            }

            error_highlighting = source_lines;
        }

        write!(f, "{}", error_highlighting)
    }
}
impl error::Error for SemanticError {
    fn description(&self) -> &str {
        "Semantic Error"
    }

    fn cause(&self) -> Option<&dyn error::Error> {
        None
    }
}

/// Utility function for extracting string literals from ast expessions
fn expect_ast_string(ast: &ast::ValueExpr, source: &str) -> Result<String, SemanticError> {
    ast.as_string(source)
        .map_err(|_| SemanticError::error_from_ast(ast, format!("Expected string literal")))
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueExpr {
    // Indirect value
    FunctionCall(FunctionCall),
    Var(String, Vec<String>),

    // Constants
    ConstFloat(f32),
    ConstLinColor(LinearRGBA),
    ConstString(String),
    ConstDict(HashMap<String, ValueExpr>),

    // Operators
    BinaryOp(BinaryOperator, Box<ValueExpr>, Box<ValueExpr>),
}

impl ValueExpr {
    pub fn from_ast(source: &str, ast: &ast::ValueExpr) -> Result<Self, SemanticError> {
        match ast {
            ast::ValueExpr::FloatLiteral(_, v) => Ok(ValueExpr::ConstFloat(*v)),
            ast::ValueExpr::ColorLiteral(_, c) => Ok(ValueExpr::ConstLinColor(*c)),
            ast::ValueExpr::StringLiteral(s) => Ok(ValueExpr::ConstString(s.to_owned(source))),
            ast::ValueExpr::Var(var) => Ok(ValueExpr::Var(var.to_owned(source), Vec::new())),
            ast::ValueExpr::PropertyOf(_, v, props) => {
                let v = ValueExpr::from_ast(source, v)?;
                if let ValueExpr::Var(v, mut p) = v {
                    p.extend(props.iter().map(|x| x.to_slice(source).to_owned()));
                    Ok(ValueExpr::Var(v, p))
                } else {
                    Err(SemanticError::error_from_ast(
                        ast,
                        format!("The `.` operator can only be used with variable names"),
                    ))
                }
            }
            ast::ValueExpr::Dictionary(d) => Ok(ValueExpr::ConstDict(
                d.entries
                    .iter()
                    .map(|kv| Ok((kv.key.to_owned(source), ValueExpr::from_ast(source, &kv.value)?)))
                    .collect::<Result<HashMap<String, ValueExpr>, SemanticError>>()?,
            )),
            ast::ValueExpr::FunctionCall(function_call) => {
                let args: Result<Vec<ValueExpr>, SemanticError> = function_call
                    .args
                    .iter()
                    .map(|e| ValueExpr::from_ast(source, e))
                    .collect();
                let args = args?;

                Ok(ValueExpr::FunctionCall(FunctionCall {
                    function: function_call.function.to_owned(source),
                    args: args,
                }))
            }
            ast::ValueExpr::BinaryOp(_, op, l, r) => {
                let l = ValueExpr::from_ast(source, l)?;
                let r = ValueExpr::from_ast(source, r)?;
                Ok(ValueExpr::BinaryOp(op.clone(), Box::new(l), Box::new(r)))
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct TextureDef {
    pub path: String,
    pub srgb: bool,
}

#[derive(Debug, PartialEq)]
pub struct IblDef {
    pub folder: String,
}

#[derive(Debug, PartialEq)]
pub struct RenderTargetDef {
    pub name: String,

    pub width: ValueExpr,
    pub height: ValueExpr,
    pub formats: Vec<(String, RenderTargetFormat)>,
    pub has_depth: bool,
}
impl RenderTargetDef {
    pub fn from_ast(source: &str, op: &ast::RenderTargetDef) -> Result<Self, SemanticError> {
        Ok(RenderTargetDef {
            name: op.name.to_slice(source).to_owned(),

            width: ValueExpr::from_ast(source, &op.width)?,
            height: ValueExpr::from_ast(source, &op.height)?,
            formats: op.formats.iter().map(|f| (f.0.to_owned(source), f.1)).collect(),
            has_depth: op.has_depth,
        })
    }
}

#[derive(Debug, Hash, Eq, PartialEq)]
pub struct ProgramDef {
    pub vert: Option<String>,
    pub tess_ctrl: Option<String>,
    pub tess_eval: Option<String>,
    pub geom: Option<String>,
    pub frag: Option<String>,
    pub comp: Option<String>,
}
impl ProgramDef {
    pub fn from_ast(source: &str, op: &ast::ValueExpr) -> Result<Self, SemanticError> {
        let mut program = ProgramDef {
            vert: None,
            tess_ctrl: None,
            tess_eval: None,
            geom: None,
            frag: None,
            comp: None,
        };

        let dict = &op
            .as_dictionary()
            .map_err(|_| SemanticError::error_from_ast(op, format!("Expected dict")))?
            .entries;
        for kv in dict {
            let shader_type = kv.key.to_slice(source);
            let shader_source = expect_ast_string(&kv.value, source)?;
            match shader_type.as_ref() {
                "vert" => program.vert = Some(shader_source.to_owned()),
                "frag" => program.frag = Some(shader_source.to_owned()),
                _ => {
                    return Err(SemanticError::error_from_ast(
                        &kv.key,
                        format!("WARNING: Unknown shader type: {}", shader_type),
                    ))
                }
            }
        }

        if program.vert.is_none() || program.frag.is_none() {
            return Err(SemanticError::error_from_ast(
                op,
                format!("vert and frag shaders are mandatory!"),
            ));
        }
        return Ok(program);
    }
}

pub struct ProgramHeader {
    sync_tracks: HashSet<String>,
    target_defs: Vec<RenderTargetDef>,
    program_defs: Vec<ProgramDef>,
    model_defs: Vec<String>,
    texture_defs: Vec<TextureDef>,
    ibl_defs: Vec<IblDef>,
    external_res: HashSet<String>,
}
impl ProgramHeader {
    pub fn new() -> Self {
        ProgramHeader {
            sync_tracks: HashSet::new(),

            target_defs: Vec::new(),
            program_defs: Vec::new(),
            model_defs: Vec::new(),
            texture_defs: Vec::new(),
            ibl_defs: Vec::new(),
            external_res: HashSet::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCall {
    pub function: String,
    pub args: Vec<ValueExpr>, // TODO: General expr type...
}

#[derive(Debug)]
pub enum BytecodeOp {
    BindRt(u32),
    BindScreenRt,
    BindProgram(u32),

    Viewport(ValueExpr, ValueExpr, ValueExpr, ValueExpr), // f32, f32, f32, f32
    Clear(ValueExpr),                                     // color

    PipelineSetBlending(u32, BlendMode),        // buffer, blending
    PipelineSetWriteMask(ValueExpr, ValueExpr), // write_color, write_depth
    PipelineSetZTest(ZTestMode),
    PipelineSetCulling(CullingMode),

    UniformFloat(String, ValueExpr),
    UniformColor(String, ValueExpr),
    UniformTexture(String, u32),
    UniformIbl(u32),
    UniformRt(String, u32, u32),

    DrawQuad,
    DrawModel(u32),

    FunctionCall(FunctionCall),
    Return {
        expr: ValueExpr,
    },

    Conditional {
        condition: ValueExpr,
        a: BlockBytecode,
        b: Option<BlockBytecode>,
    },
}

#[derive(Debug)]
pub struct BlockBytecode {
    bytecode: Vec<BytecodeOp>,
}
impl BlockBytecode {
    pub fn from_ast(source: &str, block: &Vec<Stmt>, header: &ProgramHeader) -> Result<Self, SemanticError> {
        let mut bytecode = BlockBytecode { bytecode: Vec::new() };

        for op in block {
            match op {
                ast::Stmt::FunctionCall(function_call) => {
                    if function_call.function.to_slice(source) == "program" {
                        bytecode.emit_program_bind(source, function_call, &header.program_defs)?;
                    } else if function_call.function.to_slice(source) == "bind_rt" {
                        bytecode.emit_target_bind(source, function_call, &header.target_defs)?;
                    } else if function_call.function.to_slice(source) == "pipeline_set_blending" {
                        bytecode.emit_pipeline_set_blending(source, function_call, &header.target_defs)?;
                    } else if function_call.function.to_slice(source) == "pipeline_set_write_mask" {
                        bytecode.emit_pipeline_set_write_mask(source, function_call)?;
                    } else if function_call.function.to_slice(source) == "pipeline_set_ztest" {
                        bytecode.emit_pipeline_set_ztest(source, function_call)?;
                    } else if function_call.function.to_slice(source) == "pipeline_set_culling" {
                        bytecode.emit_pipeline_set_culling(source, function_call)?;
                    } else if function_call.function.to_slice(source) == "uniform_float" {
                        Self::expect_args_count(function_call, 2)?;
                        bytecode.bytecode.push(BytecodeOp::UniformFloat(
                            expect_ast_string(&function_call.args[0], source)?,
                            ValueExpr::from_ast(source, &function_call.args[1])?,
                        ));
                    } else if function_call.function.to_slice(source) == "uniform_color" {
                        Self::expect_args_count(function_call, 2)?;
                        bytecode.bytecode.push(BytecodeOp::UniformColor(
                            expect_ast_string(&function_call.args[0], source)?,
                            ValueExpr::from_ast(source, &function_call.args[1])?,
                        ));
                    } else if function_call.function.to_slice(source) == "uniform_texture_srgb" {
                        bytecode.emit_uniform_texture(source, function_call, &header.texture_defs, true)?;
                    } else if function_call.function.to_slice(source) == "uniform_texture_linear" {
                        bytecode.emit_uniform_texture(source, function_call, &header.texture_defs, false)?;
                    } else if function_call.function.to_slice(source) == "uniform_ibl" {
                        bytecode.emit_uniform_ibl(source, function_call, &header.ibl_defs)?;
                    } else if function_call.function.to_slice(source) == "uniform_rtt" {
                        bytecode.emit_uniform_render_target_as_texture(source, function_call, &header.target_defs)?
                    } else if function_call.function.to_slice(source) == "draw_fullscreenquad" {
                        bytecode.bytecode.push(BytecodeOp::DrawQuad);
                    } else if function_call.function.to_slice(source) == "draw_model" {
                        bytecode.emit_draw_model(source, function_call, &header.model_defs)?;
                    } else if function_call.function.to_slice(source) == "clear" {
                        Self::expect_args_count(function_call, 1)?;
                        let linear = ValueExpr::from_ast(source, &function_call.args[0])?;
                        bytecode.bytecode.push(BytecodeOp::Clear(linear));
                    } else if function_call.function.to_slice(source) == "viewport" {
                        Self::expect_args_count(function_call, 4)?;
                        let x = ValueExpr::from_ast(source, &function_call.args[0])?;
                        let y = ValueExpr::from_ast(source, &function_call.args[1])?;
                        let w = ValueExpr::from_ast(source, &function_call.args[2])?;
                        let h = ValueExpr::from_ast(source, &function_call.args[3])?;
                        bytecode.emit_viewport(x, y, w, h);
                    } else {
                        bytecode.emit_function_call(source, &function_call.function, &function_call.args)?;
                    }
                }
                ast::Stmt::Return { expr } => bytecode.bytecode.push(BytecodeOp::Return {
                    expr: ValueExpr::from_ast(source, expr)?,
                }),

                ast::Stmt::Conditional { condition, a, b } => {
                    let condition = ValueExpr::from_ast(source, condition)?;
                    let a = BlockBytecode::from_ast(source, a, header)?;
                    let b = b
                        .as_ref()
                        .map(|b| BlockBytecode::from_ast(source, b, header))
                        .transpose()?;
                    bytecode.bytecode.push(BytecodeOp::Conditional {
                        condition: condition,
                        a: a,
                        b: b,
                    });
                }
            }
        }

        Ok(bytecode)
    }

    pub fn get_bytecode(&self) -> &Vec<BytecodeOp> {
        &self.bytecode
    }

    fn expect_args_count(function_call: &ast::FunctionCallExpr, args_count: usize) -> Result<(), SemanticError> {
        if function_call.args.len() == args_count {
            Ok(())
        } else {
            Err(SemanticError::error_from_ast(
                function_call,
                format!(
                    "Expected {} arguments, but got {}.",
                    args_count,
                    function_call.args.len()
                ),
            ))
        }
    }

    fn emit_viewport(&mut self, x: ValueExpr, y: ValueExpr, width: ValueExpr, height: ValueExpr) {
        self.bytecode.push(BytecodeOp::Viewport(x, y, width, height));
    }
    fn emit_target_bind(
        &mut self,
        source: &str,
        function_call: &ast::FunctionCallExpr,
        target_defs: &Vec<RenderTargetDef>,
    ) -> Result<(), SemanticError> {
        Self::expect_args_count(function_call, 1)?;
        let name = expect_ast_string(&function_call.args[0], source)?;
        if name == "screen" {
            self.bytecode.push(BytecodeOp::BindScreenRt);
            return Ok(());
        }

        let idx = target_defs.iter().position(|t| t.name == name);
        idx.map(|idx| self.bytecode.push(BytecodeOp::BindRt(idx as u32)))
            .ok_or_else(|| {
                SemanticError::error_from_ast(
                    &function_call.args[0],
                    format!("Trying to bind unknown render target {:?}", name),
                )
            })
    }
    fn emit_pipeline_set_blending(
        &mut self,
        source: &str,
        function_call: &ast::FunctionCallExpr,
        target_defs: &Vec<RenderTargetDef>,
    ) -> Result<(), SemanticError> {
        Self::expect_args_count(function_call, 2)?;
        let render_target = expect_ast_string(&function_call.args[1], source)?;

        let buffer_idx = if render_target == "screen" {
            0
        } else {
            let parts: Vec<&str> = render_target.split('.').collect();
            if parts.len() != 2 {
                return Err(SemanticError::error_from_ast(
                    &function_call.args[1],
                    format!("The name `{:?}` is not valid: use target.buffer", render_target),
                ));
            }
            let idx = target_defs.iter().position(|t| t.name == parts[0]).ok_or_else(|| {
                SemanticError::error_from_ast(
                    &function_call.args[1],
                    format!("Trying to set blending for unknown render target {:?}", render_target),
                )
            })?;
            let buffer_idx = target_defs[idx]
                .formats
                .iter()
                .position(|f| f.0 == parts[1])
                .ok_or_else(|| {
                    SemanticError::error_from_ast(
                        &function_call.args[1],
                        format!("Trying to set blending for unknown buffer {:?}", render_target),
                    )
                })?;
            buffer_idx
        };

        let mode = expect_ast_string(&function_call.args[0], source)?;
        let mode = BlendMode::from_str(&mode).ok_or_else(|| {
            SemanticError::error_from_ast(&function_call.args[0], format!("Not a valid blend mode: {}", mode))
        })?;

        self.bytecode
            .push(BytecodeOp::PipelineSetBlending(buffer_idx as u32, mode));
        Ok(())
    }
    fn emit_pipeline_set_write_mask(
        &mut self,
        source: &str,
        function_call: &ast::FunctionCallExpr,
    ) -> Result<(), SemanticError> {
        Self::expect_args_count(function_call, 2)?;
        let write_color = ValueExpr::from_ast(source, &function_call.args[0])?;
        let write_depth = ValueExpr::from_ast(source, &function_call.args[1])?;

        self.bytecode
            .push(BytecodeOp::PipelineSetWriteMask(write_color, write_depth));
        Ok(())
    }
    fn emit_pipeline_set_ztest(
        &mut self,
        source: &str,
        function_call: &ast::FunctionCallExpr,
    ) -> Result<(), SemanticError> {
        Self::expect_args_count(function_call, 1)?;
        let mode = expect_ast_string(&function_call.args[0], source)?;
        let mode = ZTestMode::from_str(&mode).ok_or_else(|| {
            SemanticError::error_from_ast(&function_call.args[0], format!("Not a valid z-test mode: {}", mode))
        })?;

        self.bytecode.push(BytecodeOp::PipelineSetZTest(mode));
        Ok(())
    }

    fn emit_pipeline_set_culling(
        &mut self,
        source: &str,
        function_call: &ast::FunctionCallExpr,
    ) -> Result<(), SemanticError> {
        Self::expect_args_count(function_call, 1)?;
        let mode = expect_ast_string(&function_call.args[0], source)?;
        let mode = CullingMode::from_str(&mode).ok_or_else(|| {
            SemanticError::error_from_ast(&function_call.args[0], format!("Not a valid culling mode: {}", mode))
        })?;

        self.bytecode.push(BytecodeOp::PipelineSetCulling(mode));
        Ok(())
    }

    fn emit_program_bind(
        &mut self,
        source: &str,
        function_call: &ast::FunctionCallExpr,
        program_defs: &Vec<ProgramDef>,
    ) -> Result<(), SemanticError> {
        Self::expect_args_count(function_call, 1)?;
        let prog = ProgramDef::from_ast(source, &function_call.args[0])?;
        let idx = program_defs.iter().position(|d| *d == prog).unwrap();

        self.bytecode.push(BytecodeOp::BindProgram(idx as u32));
        Ok(())
    }
    fn emit_draw_model(
        &mut self,
        source: &str,
        function_call: &ast::FunctionCallExpr,
        model_defs: &Vec<String>,
    ) -> Result<(), SemanticError> {
        Self::expect_args_count(function_call, 1)?;
        let model_file = expect_ast_string(&function_call.args[0], source)?;
        let idx = model_defs.iter().position(|d| *d == model_file).unwrap();

        self.bytecode.push(BytecodeOp::DrawModel(idx as u32));
        Ok(())
    }
    fn emit_uniform_texture(
        &mut self,
        source: &str,
        function_call: &ast::FunctionCallExpr,
        texture_defs: &Vec<TextureDef>,
        srgb: bool,
    ) -> Result<(), SemanticError> {
        Self::expect_args_count(function_call, 2)?;
        let texture_file = expect_ast_string(&function_call.args[1], source)?;
        let texture_def = TextureDef {
            path: texture_file,
            srgb: srgb,
        };
        let idx = texture_defs.iter().position(|d| *d == texture_def).unwrap();

        self.bytecode.push(BytecodeOp::UniformTexture(
            expect_ast_string(&function_call.args[0], source)?,
            idx as u32,
        ));
        Ok(())
    }
    fn emit_uniform_ibl(
        &mut self,
        source: &str,
        function_call: &ast::FunctionCallExpr,
        ibl_defs: &Vec<IblDef>,
    ) -> Result<(), SemanticError> {
        Self::expect_args_count(function_call, 1)?;
        let folder = expect_ast_string(&function_call.args[0], source)?;
        let ibl_def = IblDef { folder: folder };
        let idx = ibl_defs.iter().position(|d| *d == ibl_def).unwrap();

        self.bytecode.push(BytecodeOp::UniformIbl(idx as u32));

        Ok(())
    }
    fn emit_uniform_render_target_as_texture(
        &mut self,
        source: &str,
        function_call: &ast::FunctionCallExpr,
        target_defs: &Vec<RenderTargetDef>,
    ) -> Result<(), SemanticError> {
        let uniform_name = expect_ast_string(&function_call.args[0], source)?;
        let render_target = expect_ast_string(&function_call.args[1], source)?;

        let parts: Vec<&str> = render_target.split('.').collect();
        if parts.len() != 2 {
            return Err(SemanticError::error_from_ast(
                &function_call.args[1],
                format!("The name `{:?}` is not valid: use target.buffer", render_target),
            ));
        }

        let idx = target_defs.iter().position(|t| t.name == parts[0]).ok_or_else(|| {
            SemanticError::error_from_ast(
                &function_call.args[1],
                format!("Trying to bind unknown render target {:?} as texture", render_target),
            )
        })?;

        let buffer_idx = target_defs[idx]
            .formats
            .iter()
            .position(|f| f.0 == parts[1])
            .ok_or_else(|| {
                SemanticError::error_from_ast(
                    &function_call.args[1],
                    format!("Trying to bind unknown buffer {:?} as texture", render_target),
                )
            })?;

        self.bytecode
            .push(BytecodeOp::UniformRt(uniform_name, idx as u32, buffer_idx as u32));

        Ok(())
    }

    fn emit_function_call(
        &mut self,
        source: &str,
        function: &ast::SourceSlice,
        args: &Vec<ast::ValueExpr>,
    ) -> Result<(), SemanticError> {
        let args: Result<Vec<ValueExpr>, SemanticError> = args.iter().map(|e| ValueExpr::from_ast(source, e)).collect();
        let args = args?;
        self.bytecode.push(BytecodeOp::FunctionCall(FunctionCall {
            function: function.to_owned(source),
            args: args,
        }));
        Ok(())
    }
}

pub struct Function {
    pub name: String,
    pub params: Vec<(String, ast::Type)>,
    pub bytecode: BlockBytecode,
}
impl Function {
    pub fn from_ast(source: &str, ast: &ast::Function, header: &ProgramHeader) -> Result<Self, SemanticError> {
        let bytecode = BlockBytecode::from_ast(source, &ast.block, header)?;
        let params = ast
            .params
            .iter()
            .map(|p| (p.name.to_owned(source), p.value_type))
            .collect();

        Ok(Function {
            name: ast.name.to_owned(source),
            params: params,
            bytecode: bytecode,
        })
    }
}

pub struct ProgramContainer {
    header: ProgramHeader,

    // Bytecode
    functions: HashMap<String, Function>,
}

impl ProgramContainer {
    pub fn from_ast(source: &str, ast: &ast::Program) -> Result<Self, SemanticError> {
        let mut header = ProgramHeader::new();
        header.sync_tracks = Self::collect_sync_tracks(source, ast);
        header.target_defs = Self::collect_target_defs(source, ast)?;
        header.program_defs = Self::collect_program_defs(source, ast)?;
        header.model_defs = Self::collect_model_defs(source, ast)?;
        header.texture_defs = Self::collect_texture_defs(source, ast)?;
        header.ibl_defs = Self::collect_ibl_defs(source, ast)?;
        header.external_res =
            Self::collect_external_resources(&header.program_defs, &header.model_defs, &header.texture_defs);
        println!(" ~ Sync Tracks:     {:?}", header.sync_tracks.len());
        println!(" ~ Render Targets:  {:?}", header.target_defs.len());
        println!(" ~ Programs:        {:?}", header.program_defs.len());
        println!(" ~ Models:          {:?}", header.model_defs.len());
        println!(" ~ Textures:        {:?}", header.texture_defs.len());
        println!(" ~ Resources:       {:?}", header.external_res.len());

        let mut functions = HashMap::new();
        println!(" ~ Functions:       {:?}", ast.functions.len());
        for function in &ast.functions {
            let name = function.name.to_owned(source);
            let function = Function::from_ast(source, &function, &header)?;
            functions.insert(name, function);
        }

        Ok(ProgramContainer { header, functions })
    }

    pub fn get_sync_tracks(&self) -> &HashSet<String> {
        &self.header.sync_tracks
    }

    pub fn get_target_defs(&self) -> &Vec<RenderTargetDef> {
        &self.header.target_defs
    }

    pub fn get_program_defs(&self) -> &Vec<ProgramDef> {
        &self.header.program_defs
    }

    pub fn get_model_defs(&self) -> &[String] {
        &self.header.model_defs
    }

    pub fn get_texture_defs(&self) -> &[TextureDef] {
        &self.header.texture_defs
    }

    pub fn get_ibl_defs(&self) -> &[IblDef] {
        &self.header.ibl_defs
    }

    pub fn get_function(&self, function: &str) -> Option<&Function> {
        self.functions.get(function)
    }

    pub fn get_ops(&self, function: &str) -> Option<&BlockBytecode> {
        self.functions.get(function).map(|f| &f.bytecode)
    }

    fn walk_render_ops<F>(ast: &ast::Program, mut f: F) -> Result<(), SemanticError>
    where
        F: FnMut(&ast::Stmt) -> Result<(), SemanticError>,
    {
        for function in &ast.functions {
            for op in &function.block {
                f(op)?;
            }
        }
        Ok(())
    }

    fn collect_sync_tracks(source: &str, ast: &ast::Program) -> HashSet<String> {
        let mut tracks = HashSet::new();

        ast.visit_sync_tracks(source, &mut |t| {
            tracks.insert(t.to_owned());
        });
        tracks
    }

    fn collect_target_defs(source: &str, ast: &ast::Program) -> Result<Vec<RenderTargetDef>, SemanticError> {
        let mut result = Vec::new();
        for op in &ast.render_targets {
            if op.name.to_slice(source) == "screen" {
                return Err(SemanticError::error_from_ast(
                    op,
                    "The render target name `screen` is reserved for the window's buffer".to_owned(),
                ));
            }

            let program_def = RenderTargetDef::from_ast(source, op)?;
            if result.iter().any(|r: &RenderTargetDef| r.name == program_def.name) {
                return Err(SemanticError::error_from_ast(
                    op,
                    format!("Multiple definitions of `{}` found", program_def.name),
                ));
            }
            result.push(program_def);
        }
        Ok(result)
    }
    fn collect_program_defs(source: &str, ast: &ast::Program) -> Result<Vec<ProgramDef>, SemanticError> {
        let mut result = Vec::new();
        Self::walk_render_ops(ast, |render_op| {
            if let ast::Stmt::FunctionCall(call) = render_op {
                if call.function.to_slice(source) == "program" && call.args.len() == 1 {
                    let program_def = ProgramDef::from_ast(source, &call.args[0])?;
                    if !result.iter().any(|d: &ProgramDef| *d == program_def) {
                        result.push(program_def);
                    }
                }
            }
            Ok(())
        })?;
        Ok(result)
    }
    fn collect_model_defs(source: &str, ast: &ast::Program) -> Result<Vec<String>, SemanticError> {
        let mut result = Vec::new();
        Self::walk_render_ops(ast, |render_op| {
            if let ast::Stmt::FunctionCall(call) = render_op {
                if call.function.to_slice(source) == "draw_model" && call.args.len() == 1 {
                    let model_path = expect_ast_string(&call.args[0], source)?;
                    if !result.iter().any(|d| *d == model_path) {
                        result.push(model_path);
                    }
                }
            }
            Ok(())
        })?;
        Ok(result)
    }
    fn collect_texture_defs(source: &str, ast: &ast::Program) -> Result<Vec<TextureDef>, SemanticError> {
        let mut result = Vec::new();
        Self::walk_render_ops(ast, |render_op| {
            if let ast::Stmt::FunctionCall(call) = render_op {
                if (call.function.to_slice(source) == "uniform_texture_srgb"
                    || call.function.to_slice(source) == "uniform_texture_linear")
                    && call.args.len() == 2
                {
                    let texture_path = expect_ast_string(&call.args[1], source)?;
                    let texture_srgb = call.function.to_slice(source) == "uniform_texture_srgb";
                    let texture_def = TextureDef {
                        path: texture_path,
                        srgb: texture_srgb,
                    };
                    if !result.iter().any(|d| *d == texture_def) {
                        result.push(texture_def);
                    }
                }
            }
            Ok(())
        })?;
        Ok(result)
    }
    fn collect_ibl_defs(source: &str, ast: &ast::Program) -> Result<Vec<IblDef>, SemanticError> {
        let mut result = Vec::new();
        Self::walk_render_ops(ast, |render_op| {
            if let ast::Stmt::FunctionCall(call) = render_op {
                if call.function.to_slice(source) == "uniform_ibl" && call.args.len() == 1 {
                    let ibl_def = IblDef {
                        folder: expect_ast_string(&call.args[0], source)?,
                    };
                    if !result.iter().any(|d| *d == ibl_def) {
                        result.push(ibl_def);
                    }
                }
            }
            Ok(())
        })?;
        Ok(result)
    }
    fn collect_external_resources(
        progs: &Vec<ProgramDef>,
        models: &Vec<String>,
        textures: &Vec<TextureDef>,
    ) -> HashSet<String> {
        let mut result = HashSet::new();
        for prog in progs {
            prog.vert.as_ref().map(|p| result.insert(p.clone()));
            prog.tess_ctrl.as_ref().map(|p| result.insert(p.clone()));
            prog.tess_eval.as_ref().map(|p| result.insert(p.clone()));
            prog.geom.as_ref().map(|p| result.insert(p.clone()));
            prog.frag.as_ref().map(|p| result.insert(p.clone()));
            prog.comp.as_ref().map(|p| result.insert(p.clone()));
        }

        for model in models {
            result.insert(model.clone());
        }

        for texture in textures {
            result.insert(texture.path.clone());
        }

        result
    }
}
