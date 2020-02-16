use crate::ast;

/// Trait for traversing AST nodes and gathering information
pub trait Visitor {
    fn visit_sync_tracks<F: FnMut(&str)>(&self, source: &str, visit: &mut F);
}

impl Visitor for ast::Program {
    fn visit_sync_tracks<F: FnMut(&str)>(&self, source: &str, visit: &mut F) {
        for target_def in &self.render_targets {
            target_def.width.visit_sync_tracks(source, visit);
            target_def.height.visit_sync_tracks(source, visit);
        }

        for function in &self.functions {
            function.visit_sync_tracks(source, visit);
        }
    }
}

impl Visitor for ast::Function {
    fn visit_sync_tracks<F: FnMut(&str)>(&self, source: &str, visit: &mut F) {
        self.block.visit_sync_tracks(source, visit);
    }
}

// TODO: CodeBlock should be its own type
impl Visitor for Vec<ast::Stmt> {
    fn visit_sync_tracks<F: FnMut(&str)>(&self, source: &str, visit: &mut F) {
        for render_op in self {
            render_op.visit_sync_tracks(source, visit);
        }
    }
}

impl Visitor for ast::Stmt {
    fn visit_sync_tracks<F: FnMut(&str)>(&self, source: &str, visit: &mut F) {
        match self {
            ast::Stmt::FunctionCall(function_call) => {
                for arg in &function_call.args {
                    arg.visit_sync_tracks(source, visit);
                }
            }
            ast::Stmt::Return { expr } => {
                expr.visit_sync_tracks(source, visit);
            }
            ast::Stmt::Conditional { condition, a, b } => {
                condition.visit_sync_tracks(source, visit);
                a.visit_sync_tracks(source, visit);
                b.as_ref().map(|b| b.visit_sync_tracks(source, visit));
            }
        }
    }
}

impl Visitor for ast::ValueExpr {
    fn visit_sync_tracks<F: FnMut(&str)>(&self, source: &str, visit: &mut F) {
        match self {
            ast::ValueExpr::PropertyOf(_, p, a) => {
                if let ast::ValueExpr::Var(p) = **p {
                    if p.to_slice(source) == "sync" {
                        let a = a.iter().map(|a| a.to_owned(source)).collect::<Vec<String>>();
                        visit(&a.join(":"));
                    }
                }
            }
            ast::ValueExpr::FunctionCall(function_call) => {
                for arg in &function_call.args {
                    arg.visit_sync_tracks(source, visit);
                }
            }
            ast::ValueExpr::BinaryOp(_, _, a, b) => {
                a.visit_sync_tracks(source, visit);
                b.visit_sync_tracks(source, visit);
            }

            _ => {}
        }
    }
}
