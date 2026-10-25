use dxn_dtcg::expressions::expression_module;

struct Brand;

#[expression_module(specifier = "brand")]
impl Brand {
    pub async fn double(value: f64) -> f64 {
        value * 2.0
    }
}

fn main() {}
