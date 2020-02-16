#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,

    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum RenderTargetFormat {
    // sRGB
    Srgb8,
    Srgba8,

    // linear formats (8 bit)
    R8,
    Rgb8,
    Rgba8,

    // linear formats (16 bit)
    R16,
    R16F,
    Rgb16,
    Rgb16F,
    Rgba16,
    Rgba16F,

    // linear formats (32 bits)
    R32F,
    Rgb32F,
    Rgba32F,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum BlendMode {
    None,
    Add,
    AlphaBlend,
    OitCoverageBlend,
}

impl BlendMode {
    pub fn from_str(str_value: &str) -> Option<Self> {
        if str_value == "none" {
            Some(BlendMode::None)
        } else if str_value == "add" {
            Some(BlendMode::Add)
        } else if str_value == "alpha_blend" {
            Some(BlendMode::AlphaBlend)
        } else if str_value == "oit_coverage_blend" {
            Some(BlendMode::OitCoverageBlend)
        } else {
            None
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ZTestMode {
    LessEqual,
    Equal,
    Always,
}

impl ZTestMode {
    pub fn from_str(str_value: &str) -> Option<Self> {
        if str_value == "less_equal" {
            Some(ZTestMode::LessEqual)
        } else if str_value == "equal" {
            Some(ZTestMode::Equal)
        } else if str_value == "always" {
            Some(ZTestMode::Always)
        } else {
            None
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CullingMode {
    Front,
    Back,
    None,
}

impl CullingMode {
    pub fn from_str(str_value: &str) -> Option<Self> {
        if str_value == "front" {
            Some(CullingMode::Front)
        } else if str_value == "back" {
            Some(CullingMode::Back)
        } else if str_value == "none" {
            Some(CullingMode::None)
        } else {
            None
        }
    }
}
