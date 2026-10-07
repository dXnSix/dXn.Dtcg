use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
#[dtcg(with = "convert")]
struct Subject {
    a: String,
}

fn main() {}
