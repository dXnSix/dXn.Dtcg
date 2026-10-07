use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(path = "a", rename = "b")]
    a: String,
}

fn main() {}
