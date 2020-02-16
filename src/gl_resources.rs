use gl;
use gl::types::{GLchar, GLenum, GLfloat, GLint, GLuint, GLvoid};

use std::collections::HashMap;
use std::ffi::CString;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::mem;
use std::path::Path;
use std::ptr;

use imageio::RawImage;
use types::RenderTargetFormat;

#[derive(Debug)]
pub struct ShaderProgram {
    program_id: GLuint,
}
impl ShaderProgram {
    pub fn from_vert_frag(vert_source: &str, frag_source: &str) -> Result<Self, String> {
        let program;
        unsafe {
            let vs = Self::compile_shader(vert_source, gl::VERTEX_SHADER)?;
            let fs = Self::compile_shader(frag_source, gl::FRAGMENT_SHADER)?;

            program = gl::CreateProgram();
            gl::AttachShader(program, vs);
            gl::AttachShader(program, fs);
            gl::LinkProgram(program);
            let mut status = gl::FALSE as GLint;
            gl::GetProgramiv(program, gl::LINK_STATUS, &mut status);

            if status != (gl::TRUE as GLint) {
                let mut len: GLint = 0;
                gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);
                let mut buf = Vec::with_capacity(len as usize);
                buf.set_len((len as usize) - 1);
                gl::GetProgramInfoLog(program, len, ptr::null_mut(), buf.as_mut_ptr() as *mut GLchar);

                return Err(format!("Failed to link:\n{}", String::from_utf8(buf).unwrap()));
            }
        }

        Ok(ShaderProgram { program_id: program })
    }

    pub fn bind(&self) {
        unsafe {
            gl::UseProgram(self.program_id);
        }
    }

    pub fn get_uniform_location(&self, uniform_name: &str) -> Option<GLint> {
        let loc;
        unsafe {
            loc = gl::GetUniformLocation(self.program_id, CString::new(uniform_name).unwrap().as_ptr());
        }
        if loc != -1 {
            Some(loc)
        } else {
            None
        }
    }

    fn compile_shader(src: &str, shader_type: GLenum) -> Result<GLuint, String> {
        unsafe {
            let mut status = gl::FALSE as GLint;
            let shader = gl::CreateShader(shader_type);
            let src = CString::new(src).unwrap();

            gl::ShaderSource(shader, 1, &src.as_ptr(), ptr::null());
            gl::CompileShader(shader);
            gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut status);
            if status != (gl::TRUE as GLint) {
                let mut len: GLint = 0;
                gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut len);
                let mut buf = Vec::with_capacity(len as usize);
                buf.set_len((len as usize) - 1);
                gl::GetShaderInfoLog(shader, len, ptr::null_mut(), buf.as_mut_ptr() as *mut GLchar);

                return Err(format!("Failed to compile shader {}", String::from_utf8(buf).unwrap()));
            }

            Ok(shader)
        }
    }
}
impl Drop for ShaderProgram {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.program_id);
        }
    }
}

#[derive(Debug)]
pub struct RenderTarget {
    fbo_handle: GLuint,
    textures: Vec<GLuint>,
    depth_buf: Option<GLuint>,
    width: u32,
    height: u32,
}
impl RenderTarget {
    pub fn new(width: u32, height: u32, has_depth: bool, formats: &[RenderTargetFormat]) -> Result<Self, String> {
        if formats.len() > 4 {
            return Err(format!(
                "Only up to 4 color buffers are supported, you provided {}",
                formats.len()
            ));
        }

        let mut fbo_handle: GLuint = 0;
        let mut textures = Vec::new();
        let mut depth_buf: Option<GLuint> = None;
        unsafe {
            gl::GenFramebuffers(1, &mut fbo_handle);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo_handle);

            textures.resize(formats.len(), 0);
            gl::GenTextures(formats.len() as GLint, textures.as_mut_ptr());

            // Generate the color buffers
            for (i, fmt) in formats.iter().enumerate() {
                gl::ActiveTexture(gl::TEXTURE0 + i as GLuint);
                gl::BindTexture(gl::TEXTURE_2D, textures[i]);
                gl::TexStorage2D(gl::TEXTURE_2D, 1, Self::to_gl_format(*fmt), width as i32, height as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);

                gl::FramebufferTexture2D(
                    gl::FRAMEBUFFER,
                    gl::COLOR_ATTACHMENT0 + i as GLuint,
                    gl::TEXTURE_2D,
                    textures[i],
                    0,
                );
            }

            // Optionally generate the depth stencil
            if has_depth {
                let mut depth_buf_id = 0;
                gl::GenRenderbuffers(1, &mut depth_buf_id);
                gl::BindRenderbuffer(gl::RENDERBUFFER, depth_buf_id);
                gl::RenderbufferStorage(gl::RENDERBUFFER, gl::DEPTH_COMPONENT, width as i32, height as i32);
                gl::FramebufferRenderbuffer(gl::FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::RENDERBUFFER, depth_buf_id);
                depth_buf = Some(depth_buf_id);
            }

            let attachments = [
                gl::COLOR_ATTACHMENT0,
                gl::COLOR_ATTACHMENT1,
                gl::COLOR_ATTACHMENT2,
                gl::COLOR_ATTACHMENT3,
            ];
            gl::DrawBuffers(formats.len() as i32, attachments.as_ptr());

            if gl::CheckFramebufferStatus(gl::FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
                gl::DeleteFramebuffers(1, &mut fbo_handle);
                gl::DeleteTextures(textures.len() as GLint, textures.as_mut_ptr());
                depth_buf.map(|depth_buf_id| gl::DeleteRenderbuffers(1, &depth_buf_id));
                return Err(format!(
                    "Could not create framebuffer formats={:?}, depth={:?}",
                    formats, has_depth
                ));
            }
        }

        Ok(Self {
            fbo_handle: fbo_handle,
            textures: textures,
            depth_buf: depth_buf,
            width: width,
            height: height,
        })
    }

    fn to_gl_format(format: RenderTargetFormat) -> GLenum {
        match format {
            RenderTargetFormat::Srgb8 => gl::SRGB8,
            RenderTargetFormat::Srgba8 => gl::SRGB8_ALPHA8,

            RenderTargetFormat::R8 => gl::R8,
            RenderTargetFormat::Rgb8 => gl::RGB8,
            RenderTargetFormat::Rgba8 => gl::RGBA8,

            RenderTargetFormat::R16 => gl::R16,
            RenderTargetFormat::R16F => gl::R16F,
            RenderTargetFormat::Rgb16 => gl::RGB16,
            RenderTargetFormat::Rgb16F => gl::RGB16F,
            RenderTargetFormat::Rgba16 => gl::RGBA16,
            RenderTargetFormat::Rgba16F => gl::RGBA16F,

            RenderTargetFormat::R32F => gl::R32F,
            RenderTargetFormat::Rgb32F => gl::RGB32F,
            RenderTargetFormat::Rgba32F => gl::RGBA32F,
        }
    }

    pub fn bind(&self) {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo_handle);
        }
    }

    pub fn bind_as_texture(&self, texture_unit: GLuint, index: usize) {
        unsafe {
            gl::ActiveTexture(gl::TEXTURE0 + texture_unit);
            gl::BindTexture(gl::TEXTURE_2D, self.textures[index]);
        }
    }

    pub fn get_width(&self) -> u32 {
        self.width
    }
    pub fn get_height(&self) -> u32 {
        self.height
    }
}
impl Drop for RenderTarget {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &mut self.fbo_handle);
            gl::DeleteTextures(self.textures.len() as GLint, self.textures.as_mut_ptr());
            self.depth_buf
                .map(|depth_buf_id| gl::DeleteRenderbuffers(1, &depth_buf_id));
        }
    }
}

pub struct Model {
    vbo_handle: GLuint,
    vao_handle: GLuint,
    ebo_handle: GLuint,
    trig_count: GLint,
}
impl Model {
    pub fn load_obj_file(path: &Path) -> Result<Model, ()> {
        let mut vbo = 0;
        let mut ebo = 0;
        let mut vao = 0;
        let mut trig_count = 0;

        let obj = wavefront_obj::obj::parse(std::fs::read_to_string(path).map_err(|_| ())?).map_err(|_| ())?;

        if obj.objects.len() != 1 {
            return Err(()); // Expected one object
        }

        // Resolve pos/norm/tex tuples. Each unique tuple gets its own index.
        let mut resolved_vertices: HashMap<wavefront_obj::obj::VTNIndex, u32> = HashMap::new();
        let mut indices: Vec<u32> =
            Vec::with_capacity(obj.objects[0].geometry.iter().map(|x| x.shapes.len()).sum::<usize>() * 3);
        for geometry in &obj.objects[0].geometry {
            for shape in &geometry.shapes {
                if let wavefront_obj::obj::Primitive::Triangle(a, b, c) = shape.primitive {
                    for vertex in &[a, b, c] {
                        let next_index = resolved_vertices.len() as u32;
                        let vertex_idx = resolved_vertices.entry(*vertex).or_insert(next_index);
                        indices.push(*vertex_idx);
                    }
                    trig_count += 1;
                }
            }
        }

        // Create an interleaved vertex buffer
        let mut buffer: Vec<GLfloat> = Vec::with_capacity(resolved_vertices.len() * 8);
        unsafe {
            buffer.set_len(resolved_vertices.len() * 8);
        }
        for (indices, resolved_index) in resolved_vertices {
            let pos = obj.objects[0].vertices[indices.0];
            let normal = obj.objects[0]
                .normals
                .get(indices.2.unwrap_or(0))
                .unwrap_or(&wavefront_obj::obj::Vertex { x: 0.0, y: 0.0, z: 0.0 });
            let tex = obj.objects[0]
                .tex_vertices
                .get(indices.1.unwrap_or(0))
                .unwrap_or(&wavefront_obj::obj::TVertex { u: 0.0, v: 0.0, w: 0.0 });
            buffer[resolved_index as usize * 8 + 0] = pos.x as f32;
            buffer[resolved_index as usize * 8 + 1] = pos.y as f32;
            buffer[resolved_index as usize * 8 + 2] = pos.z as f32;
            buffer[resolved_index as usize * 8 + 3] = normal.x as f32;
            buffer[resolved_index as usize * 8 + 4] = normal.y as f32;
            buffer[resolved_index as usize * 8 + 5] = normal.z as f32;
            buffer[resolved_index as usize * 8 + 6] = tex.u as f32;
            buffer[resolved_index as usize * 8 + 7] = tex.v as f32;
        }

        unsafe {
            // Create GPU buffer for vertex data
            gl::GenBuffers(1, &mut vbo);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (buffer.len() * mem::size_of::<GLfloat>()) as isize,
                mem::transmute(buffer.as_ptr()),
                gl::STATIC_DRAW,
            );

            // Create GPU buffer for indices
            gl::GenBuffers(1, &mut ebo);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);
            gl::BufferData(
                gl::ELEMENT_ARRAY_BUFFER,
                (indices.len() * mem::size_of::<u32>()) as isize,
                mem::transmute(indices.as_ptr()),
                gl::STATIC_DRAW,
            );

            // Create VAO describing the vertex attributes
            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);
            gl::EnableVertexAttribArray(0);
            gl::EnableVertexAttribArray(1);
            gl::EnableVertexAttribArray(2);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            let stride = (8 * mem::size_of::<GLfloat>()) as GLint;
            gl::VertexAttribPointer(
                0,
                3,
                gl::FLOAT,
                gl::FALSE,
                stride,
                (0 * mem::size_of::<GLfloat>()) as *const GLvoid,
            );
            gl::VertexAttribPointer(
                1,
                3,
                gl::FLOAT,
                gl::FALSE,
                stride,
                (3 * mem::size_of::<GLfloat>()) as *const GLvoid,
            );
            gl::VertexAttribPointer(
                2,
                2,
                gl::FLOAT,
                gl::FALSE,
                stride,
                (6 * mem::size_of::<GLfloat>()) as *const GLvoid,
            );
        }

        Ok(Model {
            ebo_handle: ebo,
            vao_handle: vao,
            vbo_handle: vbo,
            trig_count: trig_count,
        })
    }

    pub fn draw(&self) {
        unsafe {
            gl::BindVertexArray(self.vao_handle);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, self.ebo_handle);
            gl::DrawElements(gl::TRIANGLES, self.trig_count * 3, gl::UNSIGNED_INT, ptr::null());
        }
    }
}
impl Drop for Model {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteBuffers(1, &self.ebo_handle);
            gl::DeleteVertexArrays(1, &self.vao_handle);
            gl::DeleteBuffers(1, &self.vbo_handle);
        }
    }
}

pub struct Texture {
    handle: GLuint,
}
impl Texture {
    pub fn load_file(path: &Path, srgb: bool) -> Result<Texture, ()> {
        let mut image = RawImage::from_file(path, srgb)?;
        image.flip_y();

        let mut handle: GLuint = 0;
        unsafe {
            gl::GenTextures(1, &mut handle as *mut GLuint);
            gl::BindTexture(gl::TEXTURE_2D, handle);
            let img_ptr: *const GLvoid = image.pixel_data.as_ptr() as *const GLvoid;
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                image.internal_format as GLint,
                image.width as GLint,
                image.height as GLint,
                0,
                image.format,
                image.data_type,
                img_ptr,
            );

            // HACK: Clamp 16F textures, since they are used as LUTs
            if image.data_type == gl::HALF_FLOAT {
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            } else {
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR_MIPMAP_LINEAR as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR_MIPMAP_LINEAR as i32);
                gl::GenerateMipmap(gl::TEXTURE_2D);
            }
        }

        Ok(Texture { handle: handle })
    }

    pub fn bind(&self, texture_unit: GLuint) {
        unsafe {
            gl::ActiveTexture(gl::TEXTURE0 + texture_unit);
            gl::BindTexture(gl::TEXTURE_2D, self.handle);
        }
    }
}
impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.handle);
        }
    }
}

/// Holds information about image based lighting
///
/// This information consists of a pre-filtered environment cubemap, where each MIP level represents differen roughness
/// values. The ambient light is capture as a spherical harmonics function with 9 factors per color.
pub struct Ibl {
    irradiance_sph: [f32; 27], // 9 sph factors, rgb interleaved
    handle: GLuint,
}
impl Ibl {
    pub fn load_folder(path: &Path) -> Result<Ibl, ()> {
        let mut irradiance_sph = [0.0; 27];

        let file = File::open(path.join("sh.txt")).map_err(|_| ())?;
        let mut read_values: usize = 0;
        for line in BufReader::new(file).lines().take(9) {
            let line = line.map_err(|_| ())?;
            let re = regex::Regex::new(r"-?\d+(\.\d+)?").unwrap();
            for i in re.find_iter(&line).take(3) {
                irradiance_sph[read_values] = i.as_str().parse().map_err(|_| ())?;
                read_values += 1;
            }
        }

        if read_values < 27 {
            return Err(());
        }

        let faces = [
            (gl::TEXTURE_CUBE_MAP_POSITIVE_X, "px"),
            (gl::TEXTURE_CUBE_MAP_NEGATIVE_X, "nx"),
            (gl::TEXTURE_CUBE_MAP_POSITIVE_Y, "py"),
            (gl::TEXTURE_CUBE_MAP_NEGATIVE_Y, "ny"),
            (gl::TEXTURE_CUBE_MAP_POSITIVE_Z, "pz"),
            (gl::TEXTURE_CUBE_MAP_NEGATIVE_Z, "nz"),
        ];

        let mut textures: Vec<(usize, GLenum, RawImage)> = Vec::new();
        for i in 0..9 {
            for (target, face) in faces.iter() {
                let path = path.join(format!("m{}_{}.exr", i, face));
                let image = RawImage::from_file(&path, false);
                if let Ok(image) = image {
                    textures.push((i as usize, *target, image));
                }
            }
        }

        if textures.len() < 8 * 6 {
            return Err(());
        }

        // Create cubemap
        let mut handle: GLuint = 0;
        unsafe {
            gl::GenTextures(1, &mut handle);
            gl::BindTexture(gl::TEXTURE_CUBE_MAP, handle);
            gl::TexStorage2D(
                gl::TEXTURE_CUBE_MAP,
                (textures.len() / 6) as GLint,
                textures[0].2.internal_format,
                textures[0].2.width as GLint,
                textures[0].2.height as GLint,
            );

            for t in &textures {
                gl::TexSubImage2D(
                    t.1,
                    t.0 as GLint,
                    0,
                    0,
                    t.2.width as GLint,
                    t.2.width as GLint,
                    gl::RGB,
                    gl::HALF_FLOAT,
                    t.2.pixel_data.as_ptr() as *const GLvoid,
                );
            }

            gl::TexParameteri(
                gl::TEXTURE_CUBE_MAP,
                gl::TEXTURE_MAG_FILTER,
                gl::LINEAR_MIPMAP_LINEAR as GLint,
            );
            gl::TexParameteri(
                gl::TEXTURE_CUBE_MAP,
                gl::TEXTURE_MIN_FILTER,
                gl::LINEAR_MIPMAP_LINEAR as GLint,
            );
        }

        Ok(Ibl {
            irradiance_sph: irradiance_sph,
            handle: handle,
        })
    }

    pub fn bind(&self, texture_unit: GLuint) {
        unsafe {
            gl::ActiveTexture(gl::TEXTURE0 + texture_unit);
            gl::BindTexture(gl::TEXTURE_CUBE_MAP, self.handle);
        }
    }

    pub fn irradiance_sph(&self) -> &[f32; 27] {
        &self.irradiance_sph
    }
}
impl Drop for Ibl {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.handle);
        }
    }
}
