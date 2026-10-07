use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(rest, rename = "x")]
    a: Vec<String>,
}

fn main() {}
