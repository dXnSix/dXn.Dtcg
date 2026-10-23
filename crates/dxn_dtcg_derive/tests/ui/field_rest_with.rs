use dxn_dtcg::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(rest, with = "convert")]
    a: Vec<String>,
}

fn main() {}
