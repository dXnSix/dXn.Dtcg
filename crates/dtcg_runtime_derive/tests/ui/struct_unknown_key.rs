use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
#[dtcg(roo = "x")]
struct Subject {
    a: String,
}

fn main() {}
