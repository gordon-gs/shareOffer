use crate::session::DetailConfig;
use fproto::stream_frame::tdgw_bin::new_order_single::NewOrderSingle;
use std::sync::Arc;
#[derive(Default, Clone, PartialEq, Debug)]
pub struct RouteInfo {
    pub gw_id: u16,                      //交易网关id
    pub oms_id: u16,                     //柜台id
    pub share_offer_id: u16,             //共享报盘id
    pub route_direction: RouteDirection, //路由方向
    pub route_link_type: RouteLinkType,  //路由信息来源
    pub origin_userinfo: [u8; 32],
    pub session_config: Arc<DetailConfig>
}

#[derive(Default, Clone, PartialEq, Debug)]
pub enum RouteDirection {
    #[default]
    GW2OMS, // gw to oms route info
    OMS2GW, // oms to gw rouet info
}

#[derive(Default, Clone, PartialEq, Debug)]
pub enum RouteLinkType {
    #[default]
    Software, // software socket link
    Hardware, // fpga socket link
}

impl RouteInfo {
    pub fn new_from_tdgw_user_info(
        userinfo: &[u8; 32],
        route_direction: RouteDirection,
        route_link_type: RouteLinkType,
        detail_config: Arc<DetailConfig>
    ) -> Self {
        let mut gw_id = 0u16;
        let mut oms_id = 0u16;
        let mut share_offer_id = 0u16;
        for (index, &byte) in userinfo[0..3].iter().enumerate() {
            match index {
                0 => {
                    gw_id = byte as u16;
                }
                1 => {
                    oms_id = byte as u16;
                }
                2 => {
                    share_offer_id = byte as u16;
                }
                _ => {
                    //pass
                }
            }
        }
        let route_info = Self {
            gw_id: gw_id,
            oms_id: oms_id,
            share_offer_id: share_offer_id,
            route_direction: route_direction,
            route_link_type: route_link_type,
            origin_userinfo: userinfo.clone(),
            session_config: detail_config
        };
        route_info
    }

    pub fn new_from_tgw_user_info(
        userinfo: &[u8; 8],
        route_direction: RouteDirection,
        route_link_type: RouteLinkType,
        detail_config: Arc<DetailConfig>
    ) -> Self {
        let mut gw_id = 0u16;
        let mut oms_id = 0u16;
        let mut share_offer_id = 0u16;
        for (index, &byte) in userinfo[0..3].iter().enumerate() {
            match index {
                0 => {
                    gw_id = byte as u16;
                }
                1 => {
                    oms_id = byte as u16;
                }
                2 => {
                    share_offer_id = byte as u16;
                }
                _ => {
                    //pass
                }
            }
        }
        let mut convert_user_info = [32 as u8; 32];
        convert_user_info[..8].copy_from_slice(userinfo);
        let route_info = Self {
            gw_id: gw_id,
            oms_id: oms_id,
            share_offer_id: share_offer_id,
            route_direction: route_direction,
            route_link_type: route_link_type,
            origin_userinfo: convert_user_info,
            session_config: detail_config
        };
        route_info
    }

    pub fn get_tdgw_user_info(&self) -> [u8; 32] {
        let mut userinfo = [32u8; 32];
        userinfo[0] = self.gw_id as u8;
        userinfo[1] = self.oms_id as u8;
        userinfo[2] = self.share_offer_id as u8;
        userinfo[3..].copy_from_slice(&self.origin_userinfo[3..]);
        userinfo
    }

    pub fn get_tgw_user_info(&self) -> [u8; 8] {
        let mut userinfo = [32u8; 8];
        userinfo[0] = self.gw_id as u8;
        userinfo[1] = self.oms_id as u8;
        userinfo[2] = self.share_offer_id as u8;
        userinfo[3..].copy_from_slice(&self.origin_userinfo[3..8]);
        userinfo
    }


    pub fn set_tdgw_gw_id(&self,userinfo: &mut [u8; 32],gw_id:u8){
        userinfo[0] = gw_id
    }
}
