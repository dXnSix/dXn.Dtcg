use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(path = "x")]
    #[dtcg(path = "y")]
    a: String,
}

fn main() {}
