use dxn_dtcg::DtcgTokens;

#[derive(DtcgTokens)]
#[dtcg(with = "convert")]
struct Subject {
    a: String,
}

fn main() {}
