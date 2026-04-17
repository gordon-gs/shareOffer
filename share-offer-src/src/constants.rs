// tdgw platform_id
pub const TDGW_PLATFORM_ID_0: u16 = 0;
pub const TDGW_PLATFORM_ID_2: u16 = 2;
// tdgw platform_state
pub const TDGW_PLATFORM_STATE_NOTOPEN_0: u16 = 0;
pub const TDGW_PLATFORM_STATE_PREOPEN_1: u16 = 1;
pub const TDGW_PLATFORM_STATE_OPEN_2: u16 = 2;
pub const TDGW_PLATFORM_STATE_BREAK_3: u16 = 3;
pub const TDGW_PLATFORM_STATE_CLOSE_4: u16 = 4;
// userinfo[0] judge value
pub const USERINFO_FIRST_BIT_VALID_VALUE: u16 = 48;
// send business reject / order reject to oms when send failed
pub const SHARE_OFFER_ORDER_REJECT_CODE: u32 = 32653;

#[derive(Debug, Clone, Copy)]
pub enum IdMapType {
    A2R,
    R2A,
}

impl IdMapType {
    pub fn as_str(&self) -> &'static str {
        match self {
            IdMapType::A2R => "a2r",
            IdMapType::R2A => "r2a",
        }
    }
}

pub fn is_tdgw_ready(platform_state: u16) -> bool
{
    matches!(
        platform_state,
        TDGW_PLATFORM_STATE_PREOPEN_1
        |TDGW_PLATFORM_STATE_OPEN_2
    )
}
