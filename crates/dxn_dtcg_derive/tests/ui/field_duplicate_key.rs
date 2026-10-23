use dxn_dtcg::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(path = "x")]
    #[dtcg(path = "y")]
    a: String,
}

fn main() {}
