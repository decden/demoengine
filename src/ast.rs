use color::LinearRGBA;
use types::{BinaryOperator, RenderTargetFormat};

pub trait AstNode {
    fn source_slice(&self) -> SourceSlice;
}

/// Represents a slice of the source
///
/// In order to save on memory, the slice itself does not hold a reference to the source, but only
/// the start and end position.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct SourceSlice {
    pub begin: usize,
    pub end: usize,
}
impl SourceSlice {
    pub fn new(begin: usize, end: usize) -> SourceSlice {
        SourceSlice { begin: begin, end: end }
    }
    pub fn to_slice<'a>(&self, source: &'a str) -> &'a str {
        &source[self.begin..self.end]
    }
    pub fn to_owned(&self, source: &str) -> String {
        return self.to_slice(source).to_owned();
    }
}
impl AstNode for SourceSlice {
    fn source_slice(&self) -> SourceSlice {
        *self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCallExpr {
    pub source_slice: SourceSlice,
    pub function: SourceSlice,
    pub args: Vec<ValueExpr>,
}
impl AstNode for FunctionCallExpr {
    fn source_slice(&self) -> SourceSlice {
        self.source_slice
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyValuePairExpr {
    pub key: SourceSlice,
    pub value: ValueExpr,
}
impl KeyValuePairExpr {
    pub fn new(key: SourceSlice, value: ValueExpr) -> Self {
        Self { key: key, value: value }
    }
}
impl AstNode for KeyValuePairExpr {
    fn source_slice(&self) -> SourceSlice {
        SourceSlice::new(self.key.begin, self.value.source_slice().end)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DictionaryExpr {
    pub source_slice: SourceSlice,
    pub entries: Vec<KeyValuePairExpr>,
}
impl DictionaryExpr {
    pub fn new(source_slice: SourceSlice, entries: Vec<KeyValuePairExpr>) -> Self {
        Self {
            source_slice: source_slice,
            entries: entries,
        }
    }
}
impl AstNode for DictionaryExpr {
    fn source_slice(&self) -> SourceSlice {
        self.source_slice
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueExpr {
    Var(SourceSlice),

    FloatLiteral(SourceSlice, f32),
    ColorLiteral(SourceSlice, LinearRGBA),
    StringLiteral(SourceSlice),

    PropertyOf(SourceSlice, Box<ValueExpr>, Vec<SourceSlice>),
    Dictionary(DictionaryExpr),

    FunctionCall(FunctionCallExpr),
    BinaryOp(SourceSlice, BinaryOperator, Box<ValueExpr>, Box<ValueExpr>),
}
impl ValueExpr {
    pub fn as_dictionary(&self) -> Result<&DictionaryExpr, ()> {
        match self {
            ValueExpr::Dictionary(dict) => Ok(dict),
            _ => Err(()),
        }
    }
    pub fn as_string(&self, source: &str) -> Result<String, ()> {
        match self {
            ValueExpr::StringLiteral(string) => Ok(string.to_owned(source)),
            _ => Err(()),
        }
    }
}
impl AstNode for ValueExpr {
    fn source_slice(&self) -> SourceSlice {
        match self {
            ValueExpr::Var(s) => *s,
            ValueExpr::FloatLiteral(s, _) => *s,
            ValueExpr::ColorLiteral(s, _) => *s,
            ValueExpr::StringLiteral(s) => *s,
            ValueExpr::PropertyOf(s, _, _) => *s,
            ValueExpr::Dictionary(d) => d.source_slice(),
            ValueExpr::FunctionCall(f) => f.source_slice(),
            ValueExpr::BinaryOp(s, _, _, _) => *s,
        }
    }
}

// Rendering operations

#[derive(Debug)]
pub struct RenderTargetDef {
    pub source_slice: SourceSlice,
    pub name: SourceSlice,
    pub width: ValueExpr,
    pub height: ValueExpr,
    pub formats: Vec<(SourceSlice, RenderTargetFormat)>,
    pub has_depth: bool,
}
impl RenderTargetDef {
    pub fn new(
        source_slice: SourceSlice,
        name: SourceSlice,
        width: ValueExpr,
        height: ValueExpr,
        formats: Vec<(SourceSlice, RenderTargetFormat)>,
        has_depth: bool,
    ) -> Self {
        Self {
            source_slice: source_slice,
            name: name,
            width: width,
            height: height,
            formats: formats,
            has_depth: has_depth,
        }
    }
}
impl AstNode for RenderTargetDef {
    fn source_slice(&self) -> SourceSlice {
        self.source_slice
    }
}

#[derive(Debug)]
pub enum Stmt {
    FunctionCall(FunctionCallExpr),
    Return {
        expr: ValueExpr,
    },
    Conditional {
        condition: ValueExpr,
        a: Vec<Stmt>,
        b: Option<Vec<Stmt>>,
    },
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Type {
    Float32,
    LinColor,
    Str,
    Void,
}

#[derive(Debug)]
pub struct Parameter {
    pub name: SourceSlice,
    pub value_type: Type,
}

#[derive(Debug)]
pub struct Function {
    pub name: SourceSlice,
    pub params: Vec<Parameter>,
    pub block: Vec<Stmt>,
    pub return_type: Option<Type>,
}
impl Function {
    pub fn new(name: SourceSlice, params: Vec<Parameter>, block: Vec<Stmt>, return_type: Option<Type>) -> Self {
        Function {
            name: name,
            params: params,
            block: block,
            return_type: return_type,
        }
    }
}

#[derive(Debug)]
pub struct Program {
    pub render_targets: Vec<RenderTargetDef>,
    pub functions: Vec<Function>,
}
impl Program {
    pub fn new() -> Self {
        Program {
            render_targets: Vec::new(),
            functions: Vec::new(),
        }
    }
}
