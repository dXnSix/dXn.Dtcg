use dxn_dtcg::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(rest, nested)]
    a: Vec<String>,
}

fn main() {}
