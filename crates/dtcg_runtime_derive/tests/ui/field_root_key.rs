use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(root = "x")]
    a: String,
}

fn main() {}
