use std::fs::File;
use std::path::Path;

use gl::types::GLenum;
use half::f16;
use image::GenericImageView;

pub struct RawImage {
    pub width: usize,
    pub height: usize,
    pub bytes_per_pixel: usize,
    pub internal_format: GLenum,
    pub format: GLenum,
    pub data_type: GLenum,
    pub pixel_data: Box<[u8]>,
}
impl RawImage {
    pub fn from_file(path: &Path, srgb_hint: bool) -> Result<Self, ()> {
        let extension = path.extension().ok_or(())?;
        if extension == "png" || extension == "jpg" {
            Self::load_using_image(path, srgb_hint)
        } else if extension == "exr" {
            Self::load_using_exr(path)
        } else {
            Err(())
        }
    }

    fn load_using_image(path: &Path, srgb_hint: bool) -> Result<Self, ()> {
        let image = image::open(path).map_err(|_| ())?;
        let width = image.width() as usize;
        let height = image.height() as usize;
        let channels = match image {
            image::DynamicImage::ImageRgb8(_) => 3,
            image::DynamicImage::ImageBgr8(_) => 3,
            image::DynamicImage::ImageRgba8(_) => 4,
            image::DynamicImage::ImageBgra8(_) => 4,
            image::DynamicImage::ImageLuma8(_) => 1,
            _ => return Err(()),
        };

        let swap_pixels = match image {
            image::DynamicImage::ImageBgr8(_) => true,
            image::DynamicImage::ImageBgra8(_) => true,
            _ => false,
        };

        // Convert bgr(a) => rgb(a)
        let mut pixels = image.raw_pixels();
        if swap_pixels {
            for y in 0..height {
                for x in 0..width {
                    let base = (y * width + x) * channels;
                    pixels.swap(base, base + 2);
                }
            }
        }

        let internal_format = match (channels, srgb_hint) {
            (1, _) => gl::R8,
            (3, true) => gl::SRGB8,
            (4, true) => gl::SRGB8_ALPHA8,
            (3, false) => gl::RGB8,
            (4, false) => gl::RGBA8,
            _ => unreachable!(),
        };
        let format = match channels {
            1 => gl::RED,
            3 => gl::RGB,
            4 => gl::RGBA,
            _ => unreachable!(),
        };
        Ok(RawImage {
            width: width,
            height: height,
            bytes_per_pixel: channels,
            internal_format: internal_format,
            format: format,
            data_type: gl::UNSIGNED_BYTE,
            pixel_data: pixels.into_boxed_slice(),
        })
    }

    pub fn load_using_exr(path: &Path) -> Result<Self, ()> {
        let mut file = File::open(path).map_err(|_| ())?;
        let mut exr_file = openexr::InputFile::new(&mut file).map_err(|_| ())?;

        let (width, height) = exr_file.header().data_dimensions();
        let width = width as usize;
        let height = height as usize;

        let zero = f16::from_f32(0.0);
        let mut image: Vec<(f16, f16, f16)> = vec![(zero, zero, zero); width * height];
        {
            let mut fb = openexr::FrameBufferMut::new(width as u32, height as u32);
            fb.insert_channels(&[("R", 0.0), ("G", 0.0), ("B", 0.0)], &mut image);
            exr_file.read_pixels(&mut fb).map_err(|_| ())?;
        }

        let channels = 3;
        let mut pixels: Vec<u8> = Vec::with_capacity(width * height * channels);
        for p in image {
            for c in [p.0, p.1, p.2].iter() {
                let c = c.to_bits();
                pixels.push((c & 0xff) as u8);
                pixels.push((c >> 8) as u8);
            }
        }

        Ok(RawImage {
            width: width,
            height: height,
            bytes_per_pixel: 2 * channels,
            internal_format: gl::RGB16F,
            format: gl::RGB,
            data_type: gl::HALF_FLOAT,
            pixel_data: pixels.into_boxed_slice(),
        })
    }

    pub fn flip_y(&mut self) {
        for y in 0..self.height / 2 {
            for x in 0..(self.width * self.bytes_per_pixel) {
                let i1 = y * self.width * self.bytes_per_pixel + x;
                let i2 = (self.height - 1 - y) * self.width * self.bytes_per_pixel + x;
                self.pixel_data.swap(i1, i2);
            }
        }
    }
}
