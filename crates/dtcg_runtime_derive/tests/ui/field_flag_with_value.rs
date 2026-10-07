use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(nested = "yes")]
    a: String,
}

fn main() {}
