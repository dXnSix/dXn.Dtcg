use crate::DtcgTokens;

pub trait DtcgComponentTokens: DtcgTokens {
    fn component_id() -> &'static str;
}
