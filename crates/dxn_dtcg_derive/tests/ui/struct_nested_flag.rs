use dxn_dtcg::DtcgTokens;

#[derive(DtcgTokens)]
#[dtcg(nested)]
struct Subject {
    a: String,
}

fn main() {}
