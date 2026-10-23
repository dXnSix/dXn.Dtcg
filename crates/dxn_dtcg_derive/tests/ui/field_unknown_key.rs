use dxn_dtcg::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(pth = "x")]
    a: String,
}

fn main() {}
