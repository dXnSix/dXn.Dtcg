use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
#[dtcg(root = "a", root = "b")]
struct Subject {
    a: String,
}

fn main() {}
