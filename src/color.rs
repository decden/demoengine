fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.0 {
        0.0
    } else if value <= 0.04045 {
        value / 12.92
    } else if value <= 1.0 {
        ((value + 0.055) / 1.055).powf(2.4)
    } else {
        1.0
    }
}

fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.0 {
        0.0
    } else if value < 0.0031308 {
        value * 12.92
    } else if value <= 1.0 {
        value.powf(1.0 / 2.4) * 1.055 - 0.055
    } else {
        1.0
    }
}

/// Linear space color with alpha
#[derive(Clone, Debug, Copy, PartialEq)]
pub struct LinearRGBA {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
impl LinearRGBA {
    pub fn from_f32(r: f32, g: f32, b: f32, a: f32) -> Self {
        LinearRGBA { r: r, g: g, b: b, a: a }
    }
}
impl From<SrgbRGBA> for LinearRGBA {
    fn from(srgb: SrgbRGBA) -> Self {
        let r = srgb_to_linear(srgb.r);
        let g = srgb_to_linear(srgb.g);
        let b = srgb_to_linear(srgb.b);
        LinearRGBA::from_f32(r, g, b, srgb.a)
    }
}

/// sRGB color with alpha (alpha is linear)
#[derive(Clone, Debug, Copy, PartialEq)]
pub struct SrgbRGBA {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
impl SrgbRGBA {
    pub fn from_f32(r: f32, g: f32, b: f32, a: f32) -> Self {
        SrgbRGBA { r: r, g: g, b: b, a: a }
    }

    pub fn from_rgba(rgba: u32) -> Self {
        let r = ((rgba >> 24) & 0xff) as f32 / 255.0;
        let g = ((rgba >> 16) & 0xff) as f32 / 255.0;
        let b = ((rgba >> 8) & 0xff) as f32 / 255.0;
        let a = ((rgba >> 0) & 0xff) as f32 / 255.0;
        SrgbRGBA { r: r, g: g, b: b, a: a }
    }
}
impl From<LinearRGBA> for SrgbRGBA {
    fn from(linear: LinearRGBA) -> Self {
        let r = linear_to_srgb(linear.r);
        let g = linear_to_srgb(linear.g);
        let b = linear_to_srgb(linear.b);
        SrgbRGBA::from_f32(r, g, b, linear.a)
    }
}
