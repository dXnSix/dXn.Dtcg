use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(rest, with = "convert")]
    a: Vec<String>,
}

fn main() {}
