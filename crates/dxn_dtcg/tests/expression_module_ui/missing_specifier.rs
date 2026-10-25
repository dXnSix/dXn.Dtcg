use dxn_dtcg::expressions::expression_module;

struct Brand;

#[expression_module]
impl Brand {
    pub fn double(value: f64) -> f64 {
        value * 2.0
    }
}

fn main() {}
