use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(nested, with = "convert")]
    a: String,
}

fn main() {}
