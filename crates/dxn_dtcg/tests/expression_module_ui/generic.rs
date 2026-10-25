use dxn_dtcg::expressions::expression_module;

struct Brand;

#[expression_module(specifier = "brand")]
impl Brand {
    pub fn double<T: Into<f64>>(value: T) -> f64 {
        value.into() * 2.0
    }
}

fn main() {}
