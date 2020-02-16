use lalrpop_util::ParseError;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use ast::SourceSlice;
use bytecode::{ProgramContainer, SourceSnippet};
use grammar::ProgramParser;
use runtime;
use runtime::RenderContext;
use sync::SyncTracker;

fn report_parse_error(lo: usize, hi: usize, message: &str, source: &str) -> String {
    format!(
        "Parser Error: {}\n\n{}",
        message,
        SourceSnippet::new(SourceSlice::new(lo, hi), source)
    )
}

pub struct DemoScene {
    render_context: RenderContext,
    bytecode: ProgramContainer,
}

impl DemoScene {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        println!("Opening demo: {:?}", path);
        assert!(path.is_file());
        let parent_dir = path.parent().unwrap();

        let mut file = File::open(path).map_err(|e| format!("Failed to open demo file: {}", e))?;
        let mut demo_src = String::new();
        file.read_to_string(&mut demo_src).unwrap();

        // Parsing => generates AST
        let ast = ProgramParser::new().parse(&demo_src).map_err(|e| match e {
            ParseError::InvalidToken { location } => report_parse_error(location, location, "Invalid token", &demo_src),
            ParseError::UnrecognizedToken { token, .. } => {
                let location = (token.0, token.2);
                report_parse_error(location.0, location.1, "Unexpected token", &demo_src)
            }
            e => report_parse_error(0, 0, &format!("{:?}", e), &demo_src),
        })?;

        // Compiling => generates Bytecode
        let bytecode = ProgramContainer::from_ast(&demo_src, &ast)
            .map_err(|e| format!("{}\n\n{}", e, e.source_snippet(&demo_src)))?;

        // Compile programs
        let mut render_context = RenderContext::new(&parent_dir);
        for program in bytecode.get_program_defs() {
            // TODO: Right now we only support vert and frag shaders
            let vert = program.vert.as_ref().ok_or_else(|| format!("Missing vertex shader"))?;
            let frag = program
                .frag
                .as_ref()
                .ok_or_else(|| format!("Missing fragment shader"))?;
            render_context.push_new_shader(&vert, &frag)?;
        }

        // Load models
        for model in bytecode.get_model_defs() {
            render_context.push_new_model(model)?;
        }

        // Load textures
        for texture in bytecode.get_texture_defs() {
            render_context.push_new_texture(&texture.path, texture.srgb)?;
        }

        // Load ibl environments
        for ibl in bytecode.get_ibl_defs() {
            render_context.push_new_ibl(&ibl.folder)?;
        }

        Ok(Self {
            render_context: render_context,
            bytecode: bytecode,
        })
    }

    pub fn get_bytecode(&self) -> &ProgramContainer {
        &self.bytecode
    }

    pub fn draw(&mut self, width: f32, height: f32, time_s: f32, sync_track: &dyn SyncTracker) -> Result<(), String> {
        runtime::execute(
            &mut self.render_context,
            &self.bytecode,
            width,
            height,
            time_s,
            sync_track,
        )
    }
}
