use dxn_dtcg::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(path = "a", rename = "b")]
    a: String,
}

fn main() {}
