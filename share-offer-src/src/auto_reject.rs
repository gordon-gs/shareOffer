use fproto::stream_frame::tdgw_bin::{TdgwBinFrame, order_reject};
use fproto::stream_frame::tgw_bin::{TgwBinFrame, business_reject};
use fproto::protocol_error::ProtocolError;
use crate::constants;
use std::sync::Arc;
use chrono::Local;

pub fn tdgw_auto_reject(frame: Arc<TdgwBinFrame>) -> Result<Arc<TdgwBinFrame>, ProtocolError> {
    // 准备OrderReject的基础数据
    let time = Local::now()
        .format("%Y%m%d%H%M%S%3f")
        .to_string()
        .parse::<i64>()
        .expect("time limit exceed");
    
    let mut order_reject = order_reject::OrderReject::default();
    order_reject.set_transact_time((time % 1_000_000_000 * 10_000) as u64);
    order_reject.set_ord_rej_reason(constants::SHARE_OFFER_ORDER_REJECT_CODE);

    // 模式匹配处理不同消息类型
    match &*frame {
        TdgwBinFrame::NewOrderSingleNew(msg) => {
            order_reject.set_cl_ord_id_from_ref(msg.get_cl_ord_id());
            order_reject.set_security_id_from_ref(msg.get_security_id());
            order_reject.set_user_info_from_ref(msg.get_user_info());
            order_reject.filled_head_and_tail();
            Ok(Arc::new(TdgwBinFrame::OrderRejectNew(order_reject)))
        }
        TdgwBinFrame::OrderRejectNew(msg) => {
            order_reject.set_cl_ord_id_from_ref(msg.get_cl_ord_id());
            order_reject.set_security_id_from_ref(msg.get_security_id());
            order_reject.set_user_info_from_ref(msg.get_user_info());
            order_reject.filled_head_and_tail();
            Ok(Arc::new(TdgwBinFrame::OrderRejectNew(order_reject)))
        }
        _ => {
            // 其他消息类型返回Skip错误
            Err(ProtocolError::Skip)
        }
    }
}
 pub fn tgw_auto_reject(frame: Arc<TgwBinFrame>) -> Result<Arc<TgwBinFrame>, ProtocolError> {
        let time = Local::now()
            .format("%Y%m%d%H%M%S%3f")
            .to_string()
            .parse::<i64>()
            .expect("time limit exceed");

        let mut business_reject = business_reject::BusinessReject::default();
        business_reject.set_transact_time(time);
        business_reject.set_business_reject_reason(constants::SHARE_OFFER_ORDER_REJECT_CODE as u16);

        match &*frame {
            TgwBinFrame::NewOrder100101New(msg) => {
                business_reject.set_business_reject_ref_id_from_ref(msg.get_cl_ord_id());
                business_reject.set_appl_id_from_ref(msg.get_appl_id());
                business_reject.set_submitting_pbuid_from_ref(msg.get_submitting_pbuid());
                business_reject.set_security_id_from_ref(msg.get_security_id());
                business_reject.filled_head_and_tail();
                Ok(Arc::new(TgwBinFrame::BusinessRejectNew(business_reject)))
            }
            TgwBinFrame::NewOrder100201New(msg) => {
                business_reject.set_business_reject_ref_id_from_ref(msg.get_cl_ord_id());
                business_reject.set_appl_id_from_ref(msg.get_appl_id());
                business_reject.set_submitting_pbuid_from_ref(msg.get_submitting_pbuid());
                business_reject.set_security_id_from_ref(msg.get_security_id());
                business_reject.filled_head_and_tail();
                Ok(Arc::new(TgwBinFrame::BusinessRejectNew(business_reject)))
            }
            TgwBinFrame::NewOrder104101New(msg) => {
                business_reject.set_business_reject_ref_id_from_ref(msg.get_cl_ord_id());
                business_reject.set_appl_id_from_ref(msg.get_appl_id());
                business_reject.set_submitting_pbuid_from_ref(msg.get_submitting_pbuid());
                business_reject.set_security_id_from_ref(msg.get_security_id());
                business_reject.filled_head_and_tail();
                Ok(Arc::new(TgwBinFrame::BusinessRejectNew(business_reject)))
            }
            TgwBinFrame::OrderCancelRequestNew(msg) => {
                business_reject.set_business_reject_ref_id_from_ref(msg.get_cl_ord_id());
                business_reject.set_appl_id_from_ref(msg.get_appl_id());
                business_reject.set_submitting_pbuid_from_ref(msg.get_submitting_pbuid());
                business_reject.set_security_id_from_ref(msg.get_security_id());
                business_reject.filled_head_and_tail();
                Ok(Arc::new(TgwBinFrame::BusinessRejectNew(business_reject)))
            }
            _ => {
                Err(ProtocolError::Skip)
            }
        }
    }
