use dxn_dtcg::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(root = "x")]
    a: String,
}

fn main() {}
