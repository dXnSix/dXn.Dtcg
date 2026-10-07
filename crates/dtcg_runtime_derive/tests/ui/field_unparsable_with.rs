use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(with = "not a path!")]
    a: String,
}

fn main() {}
