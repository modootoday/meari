//! Bounded private recording at real queue firing and actual host dispatch, never authority.
use crate::infrastructure::native_host::{HostAccessError, HostStamp, HostTurnReceipt};
use std::time::Instant;

#[derive(Clone, Debug)]
pub(crate) struct Firing {
    pub owner: u64,
    pub sequence: u64,
    pub deadline: Instant,
    pub now: Instant,
    pub scope: HostStamp,
}
#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub firing: Firing,
    pub result: Option<Result<HostTurnReceipt, HostAccessError>>,
}
#[derive(Default)]
pub(crate) struct Trace {
    pending: Vec<Firing>,
    pub records: Vec<Record>,
    pub overflow: bool,
    pub destroyed: Option<Instant>,
}
impl Trace {
    pub fn is_firing(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn fired(&mut self, firing: Firing) {
        if self.records.len() + self.pending.len() >= 64 {
            self.overflow = true;
        } else {
            self.pending.push(firing);
        }
    }
    pub fn complete(&mut self, result: Result<HostTurnReceipt, HostAccessError>) {
        for firing in self.pending.drain(..) {
            self.records.push(Record {
                firing,
                result: Some(result.clone()),
            });
        }
    }
    pub fn finish_skipped(&mut self) {
        for firing in self.pending.drain(..) {
            self.records.push(Record {
                firing,
                result: None,
            });
        }
    }
}
