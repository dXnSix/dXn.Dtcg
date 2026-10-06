use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
#[dtcg(rest)]
struct Subject {
    a: String,
}

fn main() {}
