use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
#[dtcg(nested)]
struct Subject {
    a: String,
}

fn main() {}
