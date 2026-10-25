use dxn_dtcg::expressions::expression_module;

struct Brand;

#[expression_module(specifier = "brand")]
impl Brand {
    pub fn new() -> f64 {
        1.0
    }
}

fn main() {}
