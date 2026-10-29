use super::ScreenRequest;
use crate::domain::host::HostEnvironment;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeStatus {
    Applied,
    Adjusted,
}

#[derive(Debug)]
pub struct ResizeAcknowledgement {
    pub ticket: u64,
    pub status: ResizeStatus,
    pub observed: HostEnvironment,
}

/// One request per host; only a matching, validated observation commits it.
#[derive(Default)]
pub struct ResizeTransaction {
    sequence: u64,
    pending: Option<(u64, ScreenRequest)>,
}

impl ResizeTransaction {
    pub fn begin(&mut self, request: ScreenRequest, host: &HostEnvironment) -> Result<u64, String> {
        if self.pending.is_some() {
            return Err("a resize request is already pending".into());
        }
        request.validate_for(host)?;
        let ticket = self
            .sequence
            .checked_add(1)
            .ok_or("resize ticket exhausted")?;
        self.sequence = ticket;
        self.pending = Some((ticket, request));
        Ok(ticket)
    }

    pub fn acknowledge(
        &mut self,
        ticket: u64,
        observed: HostEnvironment,
    ) -> Result<ResizeAcknowledgement, String> {
        let (pending, request) = self.pending.as_ref().ok_or("no pending resize")?;
        if *pending != ticket {
            return Err("stale resize acknowledgement".into());
        }
        observed.validate()?;
        // Check target identity, but do not require a capability to remain enabled after a request.
        let expected = match request.surface {
            super::ResizeSurface::NativeClient => crate::domain::host::HostTarget::Native,
            super::ResizeSurface::WebPreview => crate::domain::host::HostTarget::Web,
        };
        if observed.target != expected {
            return Err("resize acknowledgement target mismatch".into());
        }
        let status = if request.viewport.width == observed.layout.width
            && request.viewport.height == observed.layout.height
        {
            ResizeStatus::Applied
        } else {
            ResizeStatus::Adjusted
        };
        self.pending = None;
        Ok(ResizeAcknowledgement {
            ticket,
            status,
            observed,
        })
    }

    pub fn reject(&mut self, ticket: u64) -> Result<(), String> {
        if self.pending.as_ref().map(|(id, _)| *id) != Some(ticket) {
            return Err("stale resize rejection".into());
        }
        self.pending = None;
        Ok(())
    }
}
