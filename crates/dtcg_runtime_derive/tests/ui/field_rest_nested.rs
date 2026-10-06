use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(rest, nested)]
    a: Vec<String>,
}

fn main() {}
