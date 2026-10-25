use dxn_dtcg::expressions::expression_module;

struct Brand;

#[expression_module(specifier = "brand")]
impl Brand {
    #[expression(name = "two words")]
    pub fn twice(value: f64) -> f64 {
        value * 2.0
    }
}

fn main() {}
