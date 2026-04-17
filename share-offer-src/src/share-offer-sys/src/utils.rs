use fproto::protocol_error::ProtocolError;
use fproto::FrameResult;

pub fn test_share_offer_rc(rc: i32, reason: &str) -> FrameResult<()> {
    if rc < 0 {
        return Err(ProtocolError::TOE(rc, String::from(reason)));
    }
    Ok(())
}
