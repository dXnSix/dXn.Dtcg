use dxn_dtcg::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(nested = "yes")]
    a: String,
}

fn main() {}
