use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
struct Subject {
    #[dtcg(pth = "x")]
    a: String,
}

fn main() {}
