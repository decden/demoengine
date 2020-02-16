use crate::bytecode;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::prelude::*;
use std::mem;
use std::path::{Path, PathBuf};
use std::ptr;

use gl;
use gl::types::{GLboolean, GLfloat, GLint, GLenum, GLsizeiptr, GLuint};
use glm::{GenMat, GenSquareMat};

use ast;
use bytecode::{BytecodeOp, ProgramContainer, ValueExpr};
use color::LinearRGBA;
use gl_resources::{Ibl, Model, RenderTarget, ShaderProgram, Texture};
use sync::SyncTracker;
use types::{BinaryOperator, BlendMode, RenderTargetFormat, ZTestMode, CullingMode};

static VERTEX_DATA: [GLfloat; 8] = [-1., 1., -1., -1., 1., -1., 1., 1.];

pub struct RenderContext {
    parent_dir: PathBuf,

    shaders: Vec<ShaderProgram>,
    current_shader: Option<u32>,
    next_free_texture_unit: u32,

    render_targets: HashMap<u32, RenderTarget>,
    current_render_target: Option<u32>,
    targets_with_blending: HashSet<u32>,

    fullscreen_quad_vao: GLuint,
    models: Vec<Model>,
    textures: Vec<Texture>,
    ibls: Vec<Ibl>,

    model_matrix: glm::Mat4,
    view_matrix: glm::Mat4,
    projection_matrix: glm::Mat4,
}

#[derive(Debug, Clone)]
pub enum Value {
    Void,
    Float32(f32),
    LinColor(LinearRGBA),
    Str(String),
}
impl Value {
    pub fn as_f32(&self) -> Result<f32, String> {
        match self {
            Value::Float32(v) => Ok(*v),
            _ => Err(format!("Cannot convert {:?} to float", self)),
        }
    }

    pub fn as_linear_color(&self) -> Result<LinearRGBA, String> {
        match self {
            Value::LinColor(v) => Ok(*v),
            _ => Err(format!("Cannot convert {:?} to linear color", self)),
        }
    }

    pub fn value_type(&self) -> ast::Type {
        match self {
            Value::Void => ast::Type::Void,
            Value::Float32(_) => ast::Type::Float32,
            Value::LinColor(_) => ast::Type::LinColor,
            Value::Str(_) => ast::Type::Str,
        }
    }
}

pub struct FunctionContext<'a> {
    pub program: &'a ProgramContainer,
    pub sync_track: &'a dyn SyncTracker,
    pub globals: &'a HashMap<String, Value>,
    pub locals: HashMap<String, Value>,
}
impl<'a> FunctionContext<'a> {
    pub fn get_prop(&self, name: &str, props: &[String]) -> Result<Value, String> {
        if name == "sync" {
            let track = props.join(":");
            self.sync_track
                .get_value(&track)
                .map(|v| Value::Float32(v))
                .ok_or_else(|| format!("Could not get value for sync track \"{}\"", track))
        } else {
            if !props.is_empty() {
                return Err("Right now `.` is only supported for sync expressions".to_owned());
            }

            let value = self
                .locals
                .get(name)
                .or_else(|| self.globals.get(name))
                .map(|v| v.clone());
            value.ok_or_else(|| format!("Unknown variable {}", name))
        }
    }
}

fn identity_4() -> glm::Mat4 {
    glm::Mat4::new(
        glm::Vec4::new(1.0, 0.0, 0.0, 0.0),
        glm::Vec4::new(0.0, 1.0, 0.0, 0.0),
        glm::Vec4::new(0.0, 0.0, 1.0, 0.0),
        glm::Vec4::new(0.0, 0.0, 0.0, 1.0),
    )
}

impl RenderContext {
    pub fn new(path: &Path) -> Self {
        let mut quad_vao = 0;
        unsafe {
            // Enable linear color output for shaders
            gl::Enable(gl::FRAMEBUFFER_SRGB);
            gl::Enable(gl::DEPTH_TEST);
            gl::Enable(gl::TEXTURE_CUBE_MAP_SEAMLESS);
            gl::Enable(gl::CULL_FACE);

            gl::GenVertexArrays(1, &mut quad_vao);
            gl::BindVertexArray(quad_vao);

            let mut quad_vbo = 0;
            gl::GenBuffers(1, &mut quad_vbo);
            gl::BindBuffer(gl::ARRAY_BUFFER, quad_vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (VERTEX_DATA.len() * mem::size_of::<GLfloat>()) as GLsizeiptr,
                mem::transmute(&VERTEX_DATA[0]),
                gl::STATIC_DRAW,
            );

            let pos_attr = 0;
            gl::EnableVertexAttribArray(pos_attr as GLuint);
            gl::VertexAttribPointer(pos_attr as GLuint, 2, gl::FLOAT, gl::FALSE as GLboolean, 0, ptr::null());
        }

        Self {
            parent_dir: path.to_owned(),
            shaders: Vec::new(),
            current_shader: None,
            next_free_texture_unit: 0,

            render_targets: HashMap::new(),
            current_render_target: None,
            targets_with_blending: HashSet::new(),

            fullscreen_quad_vao: quad_vao,
            models: Vec::new(),
            textures: Vec::new(),
            ibls: Vec::new(),

            model_matrix: identity_4(),
            view_matrix: identity_4(),
            projection_matrix: identity_4(),
        }
    }

    pub fn make_target(
        &mut self,
        idx: u32,
        width: u32,
        height: u32,
        has_depth: bool,
        formats: &[(String, RenderTargetFormat)],
    ) -> Result<(), String> {
        let mut recreate_render_target = false;
        {
            let value = self.render_targets.get(&idx);
            match value {
                Some(render_target) => {
                    if render_target.get_width() != width || render_target.get_height() != height {
                        recreate_render_target = true;
                    } else {
                        render_target.bind();
                    }
                }
                None => {
                    recreate_render_target = true;
                }
            };
        }

        let formats: Vec<RenderTargetFormat> = formats.iter().map(|x| x.1).collect();

        if recreate_render_target {
            let render_target = RenderTarget::new(width, height, has_depth, &formats)?;
            render_target.bind();
            self.render_targets.remove(&idx);
            self.render_targets.insert(idx, render_target);
        }

        Ok(())
    }

    pub fn bind_render_target(&mut self, target: Option<u32>) -> Result<(), String> {
        if let Some(target) = target {
            if let Some(render_target) = self.render_targets.get(&target) {
                render_target.bind();
                self.current_render_target = Some(target);
            } else {
                return Err(format!("Unknown render target: {}", target));
            }
        } else {
            unsafe {
                gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            }
            self.current_render_target = None;
        }
        Ok(())
    }

    pub fn viewport_rect(&mut self, x: u32, y: u32, width: u32, height: u32) {
        unsafe {
            gl::Viewport(x as GLint, y as GLint, width as GLint, height as GLint);
        }
    }

    pub fn clear(&mut self, linear: LinearRGBA) {
        unsafe {
            gl::ClearColor(linear.r, linear.g, linear.b, linear.a);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        }
    }

    pub fn set_blending(&mut self, buffer: u32, mode: BlendMode) {
        unsafe {
            match mode {
                BlendMode::None => {
                    gl::BlendFunci(buffer, gl::ONE, gl::ZERO);
                    self.targets_with_blending.remove(&buffer);
                    if self.targets_with_blending.is_empty() {
                        gl::Disable(gl::BLEND);
                    }
                }
                BlendMode::Add => {
                    if self.targets_with_blending.is_empty() {
                        gl::Enable(gl::BLEND);
                    }
                    self.targets_with_blending.insert(buffer);
                    gl::BlendFunci(buffer, gl::ONE, gl::ONE);
                }
                BlendMode::AlphaBlend => {
                    if self.targets_with_blending.is_empty() {
                        gl::Enable(gl::BLEND);
                    }
                    self.targets_with_blending.insert(buffer);
                    gl::BlendFunci(buffer, gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
                }
                BlendMode::OitCoverageBlend => {
                    if self.targets_with_blending.is_empty() {
                        gl::Enable(gl::BLEND);
                    }
                    self.targets_with_blending.insert(buffer);
                    gl::BlendFunci(buffer, gl::ZERO, gl::ONE_MINUS_SRC_ALPHA);
                }
            }
        }
    }

    pub fn set_write_mask(&mut self, write_color: bool, write_depth: bool) {
        unsafe {
            gl::ColorMask(
                write_color as u8,
                write_color as u8,
                write_color as u8,
                write_color as u8,
            );
            gl::DepthMask(write_depth as u8);
        }
    }

    pub fn set_z_test(&mut self, mode: ZTestMode) {
        let mode = match mode {
            ZTestMode::LessEqual => gl::LEQUAL,
            ZTestMode::Equal => gl::EQUAL,
            ZTestMode::Always => gl::ALWAYS,
        };

        unsafe {
            gl::DepthFunc(mode);
        }
    }

    pub fn set_culling(&mut self, mode: CullingMode) {
        let mode: Option<GLenum> = match mode {
            CullingMode::Front => Some(gl::FRONT),
            CullingMode::Back => Some(gl::BACK),
            CullingMode::None => None
        };

        unsafe {
            if let Some(mode) = mode {
                gl::Enable(gl::CULL_FACE);
                gl::CullFace(mode);
            } else {
                gl::Disable(gl::CULL_FACE);
            }
        }

    }

    pub fn push_new_shader(&mut self, vert_file: &str, frag_file: &str) -> Result<(), String> {
        let path: &PathBuf = &self.parent_dir;

        let vs_src = Self::load_shader(&path.join(vert_file))?;
        let fs_src = Self::load_shader(&path.join(frag_file))?;
        let shader = ShaderProgram::from_vert_frag(&vs_src, &fs_src)?;
        self.shaders.push(shader);
        Ok(())
    }

    pub fn push_new_model(&mut self, model_file: &str) -> Result<(), String> {
        let path: &PathBuf = &self.parent_dir;

        let model = Model::load_obj_file(&path.join(model_file))
            .map_err(|_| format!("Could not load model {:?}", model_file))?;

        self.models.push(model);
        Ok(())
    }

    pub fn push_new_texture(&mut self, texture_file: &str, srgb: bool) -> Result<(), String> {
        let path: &PathBuf = &self.parent_dir;

        let texture = Texture::load_file(&path.join(texture_file), srgb)
            .map_err(|_| format!("Could not load texture {:?}", texture_file))?;

        self.textures.push(texture);
        Ok(())
    }

    pub fn push_new_ibl(&mut self, ibl_folder: &str) -> Result<(), String> {
        let path: &PathBuf = &self.parent_dir;

        let ibl = Ibl::load_folder(&path.join(ibl_folder))
            .map_err(|_| format!("Could not load ibl folder: {:?}", ibl_folder))?;

        self.ibls.push(ibl);
        Ok(())
    }

    pub fn use_shaders(&mut self, shader_id: u32) -> Result<(), String> {
        let shader = &self.shaders[shader_id as usize];
        shader.bind();

        self.current_shader = Some(shader_id);
        self.next_free_texture_unit = 0;

        // Set uniforms
        let mv = self.view_matrix * self.model_matrix;
        let mvp = self.projection_matrix * mv;
        let mv_it = &mv
            .inverse()
            .map(|m| m.transpose())
            .ok_or_else(|| format!("Model-View matrix is non-invertible"))?;
        let _ = self.set_uniform_mat4("u_ModelViewProjectionMatrix", &mvp);
        let _ = self.set_uniform_mat4("u_ModelViewMatrix", &mv);
        let _ = self.set_uniform_mat4("u_ModelViewInvTranspMatrix", &mv_it);

        Ok(())
    }

    fn load_shader(filename: &Path) -> Result<String, String> {
        let mut file = File::open(filename).map_err(|e| format!("Failed to load shader file {:?}, {}", filename, e))?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(|e| format!("Failed to read shader file: {:?}, {}", filename, e))?;
        Ok(contents)
    }

    pub fn render_fullscreen_quad(&mut self) {
        unsafe {
            gl::BindVertexArray(self.fullscreen_quad_vao);
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
        }
    }

    pub fn render_model(&mut self, model_id: u32) {
        let model = &self.models[model_id as usize];
        model.draw();
    }

    fn get_current_program_uniform_location(&self, uniform_name: &str) -> Result<GLint, String> {
        let shader = self
            .current_shader
            .as_ref()
            .map(|id| &self.shaders[*id as usize])
            .ok_or_else(|| format!("Current shader is invalid (while setting uniform '{}')", uniform_name))?;

        shader
            .get_uniform_location(uniform_name)
            .ok_or_else(|| format!("Trying to set unknown uniform '{}'", uniform_name))
    }

    pub fn set_uniform_f32(&mut self, uniform_name: &str, value: f32) -> Result<(), String> {
        let location = self.get_current_program_uniform_location(uniform_name)?;
        unsafe {
            gl::Uniform1f(location, value);
        }
        Ok(())
    }

    pub fn set_uniform_color(&mut self, uniform_name: &str, value: LinearRGBA) -> Result<(), String> {
        let location = self.get_current_program_uniform_location(uniform_name)?;
        unsafe {
            gl::Uniform4f(location, value.r, value.g, value.b, value.a);
        }
        Ok(())
    }

    pub fn set_uniform_mat4(&mut self, uniform_name: &str, value: &glm::Mat4) -> Result<(), String> {
        let location = self.get_current_program_uniform_location(uniform_name)?;
        unsafe {
            gl::UniformMatrix4fv(location, 1, gl::FALSE, mem::transmute(value));
        }
        Ok(())
    }

    pub fn set_uniform_texture_srgb(&mut self, uniform_name: &str, texture_index: u32) -> Result<(), String> {
        let location = self.get_current_program_uniform_location(uniform_name)?;
        let texture = &self.textures[texture_index as usize];

        unsafe {
            gl::Uniform1i(location, self.next_free_texture_unit as GLint);
        }
        texture.bind(self.next_free_texture_unit);
        self.next_free_texture_unit += 1;

        Ok(())
    }

    pub fn set_uniform_ibl(&mut self, ibl_index: u32) -> Result<(), String> {
        let sph_location = self.get_current_program_uniform_location("u_IblIrrandianceSph")?;
        let texture_location = self.get_current_program_uniform_location("t_IblRadianceMap")?;
        let ibl = &self.ibls[ibl_index as usize];

        unsafe {
            gl::Uniform3fv(sph_location, 9, ibl.irradiance_sph() as *const f32);
            gl::Uniform1i(texture_location, self.next_free_texture_unit as GLint);
        }

        ibl.bind(self.next_free_texture_unit);
        self.next_free_texture_unit += 1;

        Ok(())
    }

    pub fn set_uniform_render_target_texture(
        &mut self,
        uniform_name: &str,
        target_index: u32,
        buffer_index: u32,
    ) -> Result<(), String> {
        let location = self.get_current_program_uniform_location(uniform_name)?;
        let render_target = self
            .render_targets
            .get(&target_index)
            .ok_or_else(|| format!("Unknown render target at index {}", target_index))?;

        unsafe {
            gl::Uniform1i(location, self.next_free_texture_unit as GLint);
        }
        render_target.bind_as_texture(self.next_free_texture_unit, buffer_index as usize);
        self.next_free_texture_unit += 1;

        Ok(())
    }

    pub fn set_model_matrix(&mut self, m: &glm::Mat4) {
        self.model_matrix = *m;
    }
    pub fn set_view_matrix(&mut self, m: &glm::Mat4) {
        self.view_matrix = *m;
    }
    pub fn set_projection_matrix(&mut self, m: &glm::Mat4) {
        self.projection_matrix = *m;
    }
}

pub fn evaluate_expression(
    render_ctx: &mut RenderContext,
    function_ctx: &FunctionContext,
    expr: &ValueExpr,
) -> Result<Value, String> {
    match expr {
        ValueExpr::FunctionCall(function_call) => execute_function_call(render_ctx, function_ctx, function_call),
        ValueExpr::Var(name, props) => function_ctx.get_prop(&name, &props),

        ValueExpr::ConstFloat(val) => Ok(Value::Float32(*val)),
        ValueExpr::ConstLinColor(val) => Ok(Value::LinColor(*val)),
        ValueExpr::ConstString(val) => Ok(Value::Str(val.clone())),
        ValueExpr::ConstDict(_val) => Err(format!("Const dict not supported")),

        // Only implemented for floats for now
        ValueExpr::BinaryOp(operand, e1, e2) => {
            let e1 = evaluate_expression(render_ctx, function_ctx, e1)?;
            let e2 = evaluate_expression(render_ctx, function_ctx, e2)?;
            let e1 = e1.as_f32()?;
            let e2 = e2.as_f32()?;

            match operand {
                &BinaryOperator::Add => Ok(Value::Float32(e1 + e2)),
                &BinaryOperator::Sub => Ok(Value::Float32(e1 - e2)),
                &BinaryOperator::Mul => Ok(Value::Float32(e1 * e2)),
                &BinaryOperator::Div => Ok(Value::Float32(e1 / e2)),

                &BinaryOperator::Lt => Ok(Value::Float32(if e1 < e2 { 1.0 } else { 0.0 })),
                &BinaryOperator::Le => Ok(Value::Float32(if e1 <= e2 { 1.0 } else { 0.0 })),
                &BinaryOperator::Gt => Ok(Value::Float32(if e1 > e2 { 1.0 } else { 0.0 })),
                &BinaryOperator::Ge => Ok(Value::Float32(if e1 >= e2 { 1.0 } else { 0.0 })),
                &BinaryOperator::Eq => Ok(Value::Float32(if e1 == e2 { 1.0 } else { 0.0 })),
                &BinaryOperator::Ne => Ok(Value::Float32(if e1 != e2 { 1.0 } else { 0.0 })),
            }
        }
    }
}

pub fn execute(
    render_ctx: &mut RenderContext,
    program: &ProgramContainer,
    width: f32,
    height: f32,
    time_s: f32,
    sync_track: &dyn SyncTracker,
) -> Result<(), String> {
    // Initialize context
    let mut globals: HashMap<String, Value> = HashMap::new();
    globals.insert("width".into(), Value::Float32(width));
    globals.insert("height".into(), Value::Float32(height));
    globals.insert("time".into(), Value::Float32(time_s));
    let function_ctx = FunctionContext {
        program: program,
        sync_track: sync_track,
        globals: &globals,
        locals: HashMap::new(),
    };

    // Evaluate render targets
    for (idx, rt) in program.get_target_defs().iter().enumerate() {
        let width = evaluate_expression(render_ctx, &function_ctx, &rt.width)?
            .as_f32()?
            .round() as u32;
        let height = evaluate_expression(render_ctx, &function_ctx, &rt.height)?
            .as_f32()?
            .round() as u32;
        render_ctx.make_target(idx as u32, width, height, rt.has_depth, &rt.formats)?;
    }

    // Compute camera transfomration
    let eye = glm::Vec3::new(0.0, 0.0, 5.0);
    let center = glm::Vec3::new(0.0, 0.0, 0.0);
    let up = glm::Vec3::new(0.0, 1.0, 0.0);
    let view_matrix = glm::ext::look_at(eye, center, up);
    let proj_matrix = glm::ext::perspective(0.5, width / height, 0.01, 20.0);

    render_ctx.set_view_matrix(&view_matrix);
    render_ctx.set_projection_matrix(&proj_matrix);
    let rotation_axis = glm::Vec3::new(0.0, 1.0, 0.0);
    render_ctx.set_model_matrix(&glm::ext::rotate(&identity_4(), time_s * 0.5, rotation_axis));

    call_function(render_ctx, &function_ctx, "main", HashMap::new()).map(|_| {})
}

fn call_function(
    render_ctx: &mut RenderContext,
    function_ctx: &FunctionContext,
    function: &str,
    args: HashMap<String, Value>,
) -> Result<Value, String> {
    let called_fn = function_ctx
        .program
        .get_ops(&function)
        .ok_or_else(|| format!("Function {} is not defined", function))?;

    // Create new frame
    let new_frame_ctx = FunctionContext {
        program: function_ctx.program,
        sync_track: function_ctx.sync_track,
        globals: function_ctx.globals,
        locals: args,
    };

    execute_block(render_ctx, &new_frame_ctx, called_fn)
}

fn execute_function_call(
    render_ctx: &mut RenderContext,
    function_ctx: &FunctionContext,
    function_call: &bytecode::FunctionCall,
) -> Result<Value, String> {
    if function_call.function == "LinColor" {
        // TODO: Bounds checking
        let r = evaluate_expression(render_ctx, function_ctx, &function_call.args[0])?.as_f32()?;
        let g = evaluate_expression(render_ctx, function_ctx, &function_call.args[1])?.as_f32()?;
        let b = evaluate_expression(render_ctx, function_ctx, &function_call.args[2])?.as_f32()?;
        let a = evaluate_expression(render_ctx, function_ctx, &function_call.args[3])?.as_f32()?;
        return Ok(Value::LinColor(LinearRGBA::from_f32(r, g, b, a)));
    }

    let function = function_ctx
        .program
        .get_function(&function_call.function)
        .ok_or_else(|| format!("Missing function {}", function_call.function))?;

    // Make sure enough parameters are passed
    if function.params.len() != function_call.args.len() {
        return Err(format!(
            "Expected {} arguments for call to \"{}\" function. Got {}.",
            function.params.len(),
            function_call.function,
            function_call.args.len()
        ));
    }

    let mut locals = HashMap::new();
    for (p, a) in function.params.iter().zip(function_call.args.iter()) {
        let v = evaluate_expression(render_ctx, function_ctx, a)?;
        if v.value_type() != p.1 {
            return Err(format!(
                "Expected argument \"{}\" for call to \"{}\", to have type {:?}",
                p.0, function_call.function, p.1
            ));
        }
        locals.insert(p.0.clone(), v);
    }

    call_function(render_ctx, function_ctx, &function_call.function, locals)
}

fn execute_block(
    render_ctx: &mut RenderContext,
    function_ctx: &FunctionContext,
    block: &bytecode::BlockBytecode,
) -> Result<Value, String> {
    for op in block.get_bytecode() {
        match op {
            BytecodeOp::BindRt(rt_id) => render_ctx.bind_render_target(Some(*rt_id))?,
            BytecodeOp::BindScreenRt => render_ctx.bind_render_target(None)?,
            BytecodeOp::BindProgram(program_id) => {
                render_ctx.use_shaders(*program_id)?;
            }

            BytecodeOp::Viewport(x, y, width, height) => {
                let x = evaluate_expression(render_ctx, function_ctx, &x)?.as_f32()?.round() as u32;
                let y = evaluate_expression(render_ctx, function_ctx, &y)?.as_f32()?.round() as u32;
                let width = evaluate_expression(render_ctx, function_ctx, &width)?.as_f32()?.round() as u32;
                let height = evaluate_expression(render_ctx, function_ctx, &height)?
                    .as_f32()?
                    .round() as u32;
                render_ctx.viewport_rect(x, y, width, height);
            }
            BytecodeOp::Clear(linear) => {
                let linear = evaluate_expression(render_ctx, function_ctx, linear)?.as_linear_color()?;
                render_ctx.clear(linear);
            }

            BytecodeOp::PipelineSetBlending(buffer, mode) => {
                render_ctx.set_blending(*buffer, *mode);
            }
            BytecodeOp::PipelineSetWriteMask(write_color, write_depth) => {
                let write_color = evaluate_expression(render_ctx, function_ctx, write_color)?.as_f32()? > 0.0;
                let write_depth = evaluate_expression(render_ctx, function_ctx, write_depth)?.as_f32()? > 0.0;
                render_ctx.set_write_mask(write_color, write_depth);
            }
            BytecodeOp::PipelineSetZTest(mode) => {
                render_ctx.set_z_test(*mode);
            }
            BytecodeOp::PipelineSetCulling(mode) => {
                render_ctx.set_culling(*mode);
            }

            BytecodeOp::UniformFloat(uniform_name, value) => {
                let value = evaluate_expression(render_ctx, function_ctx, &value)?.as_f32()?;
                render_ctx.set_uniform_f32(&uniform_name, value)?;
            }
            BytecodeOp::UniformColor(uniform_name, value) => {
                let value = evaluate_expression(render_ctx, function_ctx, &value)?.as_linear_color()?;
                render_ctx.set_uniform_color(&uniform_name, value)?;
            }
            BytecodeOp::UniformTexture(uniform_name, texture_id) => {
                render_ctx.set_uniform_texture_srgb(uniform_name, *texture_id)?;
            }
            BytecodeOp::UniformIbl(ibl_id) => {
                render_ctx.set_uniform_ibl(*ibl_id)?;
            }
            BytecodeOp::UniformRt(uniform_name, target_id, buffer_id) => {
                render_ctx.set_uniform_render_target_texture(uniform_name, *target_id, *buffer_id)?;
            }
            BytecodeOp::DrawQuad => {
                render_ctx.render_fullscreen_quad();
            }
            BytecodeOp::DrawModel(model_id) => {
                render_ctx.render_model(*model_id);
            }
            BytecodeOp::FunctionCall(function_call) => {
                execute_function_call(render_ctx, function_ctx, function_call)?;
            }
            BytecodeOp::Return { expr } => {
                return Ok(evaluate_expression(render_ctx, function_ctx, expr)?);
            }
            BytecodeOp::Conditional { condition, a, b } => {
                let value = evaluate_expression(render_ctx, function_ctx, condition)?
                    .as_f32()
                    .unwrap();
                if value > 0.0 {
                    execute_block(render_ctx, function_ctx, a)?;
                } else if let Some(b) = b {
                    execute_block(render_ctx, function_ctx, b)?;
                }
            }
        }
    }
    Ok(Value::Void)
}
