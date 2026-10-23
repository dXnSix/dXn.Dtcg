use dxn_dtcg::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(rest, rename = "x")]
    a: Vec<String>,
}

fn main() {}
