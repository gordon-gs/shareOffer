use std::collections::HashMap;
use tracing::{debug, warn,error};


pub struct OmsReportRouter {
    contract_to_oms: HashMap<[u8;10], u16>,
    total_orders: u64,
    total_reports: u64,
    failed_routes: u64,
}

impl OmsReportRouter {
    pub fn new() -> Self {
        Self {
            contract_to_oms: HashMap::with_capacity(10000),
            total_orders: 0,
            total_reports: 0,
            failed_routes: 0,
        }
    }

    pub fn record_order(&mut self, contract_num: [u8;10], oms_id: u16) {
        self.contract_to_oms.insert(contract_num, oms_id);
        self.total_orders += 1;
    }

    pub fn route_report(&mut self, contract_num: &[u8;10]) -> Option<u16> {
        self.total_reports += 1;
        if let Some(&oms_id) = self.contract_to_oms.get(contract_num) {
            debug!(target: "business", "route report: contract_num={:?}, oms_id={}, total={}",
                   contract_num, oms_id, self.total_reports);
            Some(oms_id)
        } else {
            self.failed_routes += 1;
            warn!(target: "business", "report routing failed: contract_num={:?} not found, failures={}/{}",
                  contract_num, self.failed_routes, self.total_reports);
            None
        }
    }


    pub fn clean_oms_orders(&mut self, oms_id: u16) {
        let before_count = self.contract_to_oms.len();
        self.contract_to_oms.retain(|_, &mut id| id != oms_id);
        let cleaned = before_count - self.contract_to_oms.len();
        debug!(target: "business", "clean oms orders: oms_id={}, cleaned={}, remaining={}",
               oms_id, cleaned, self.contract_to_oms.len());
    }

    pub fn get_stats(&self) -> (u64, u64, u64, usize) {
        (
            self.total_orders,
            self.total_reports,
            self.failed_routes,
            self.contract_to_oms.len()
        )
    }

    pub fn contains_contract(&self, contract_num: &[u8;10]) -> bool {
        self.contract_to_oms.contains_key(contract_num)
    }

    pub fn get_oms_id(&self, contract_num: &[u8;10]) -> Option<u16> {
        self.contract_to_oms.get(contract_num).copied()
    }

    pub fn print_all_record(&self){
        for (k,v) in &self.contract_to_oms{
            error!("contract route map: contract:{:?},conn_id:{:?}",k,v)
        }
    }
}

impl Default for OmsReportRouter {
    fn default() -> Self {
        Self::new()
    }
}
