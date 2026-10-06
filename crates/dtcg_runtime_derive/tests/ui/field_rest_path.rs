use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(rest, path = "x")]
    a: Vec<String>,
}

fn main() {}
