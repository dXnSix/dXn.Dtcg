use dxn_dtcg::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(with = "not a path!")]
    a: String,
}

fn main() {}
