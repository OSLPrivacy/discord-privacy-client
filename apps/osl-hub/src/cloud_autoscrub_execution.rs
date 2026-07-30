//! Per-run isolation model for future cloud AutoScrub execution.
//!
//! This is only a provisioning contract. It does not contain provider code,
//! credentials, network dispatch, or a path that can make cloud processing live.

use std::fmt;

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CloudAutoScrubRunId([u8; 32]);

impl CloudAutoScrubRunId {
    pub fn new(bytes: [u8; 32]) -> Option<Self> {
        if bytes.iter().all(|byte| *byte == 0) {
            None
        } else {
            Some(Self(bytes))
        }
    }

    pub const fn commitment(&self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for CloudAutoScrubRunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CloudAutoScrubRunId")
            .field(&"<redacted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct DisposableCloudWorker {
    run_id: CloudAutoScrubRunId,
    worker_generation: u64,
}

impl DisposableCloudWorker {
    pub const fn run_id(&self) -> CloudAutoScrubRunId {
        self.run_id
    }

    pub const fn worker_generation(&self) -> u64 {
        self.worker_generation
    }

    pub fn can_accept_run(&self, run_id: CloudAutoScrubRunId) -> bool {
        self.run_id == run_id
    }
}

impl fmt::Debug for DisposableCloudWorker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DisposableCloudWorker")
            .field("run_id", &"<redacted>")
            .field("worker_generation", &self.worker_generation)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CloudAutoScrubExecutionError {
    InvalidRunId,
    ReusedRunId,
    GenerationOverflow,
}

#[derive(Debug, Default)]
pub struct CloudAutoScrubExecutionProvisioner {
    last_run_id: Option<CloudAutoScrubRunId>,
    next_generation: u64,
}

impl CloudAutoScrubExecutionProvisioner {
    pub fn provision(
        &mut self,
        run_id: CloudAutoScrubRunId,
    ) -> Result<DisposableCloudWorker, CloudAutoScrubExecutionError> {
        if self.last_run_id == Some(run_id) {
            return Err(CloudAutoScrubExecutionError::ReusedRunId);
        }
        let worker_generation = self
            .next_generation
            .checked_add(1)
            .ok_or(CloudAutoScrubExecutionError::GenerationOverflow)?;
        self.next_generation = worker_generation;
        self.last_run_id = Some(run_id);
        Ok(DisposableCloudWorker {
            run_id,
            worker_generation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_id(fill: u8) -> CloudAutoScrubRunId {
        CloudAutoScrubRunId::new([fill; 32]).expect("nonzero run id")
    }

    #[test]
    fn isolated_execution_environment_provisions_per_run_disposable_cloud_worker() {
        let mut provisioner = CloudAutoScrubExecutionProvisioner::default();
        let first_run = run_id(1);
        let second_run = run_id(2);

        let first = provisioner
            .provision(first_run)
            .expect("first run provisions");
        let second = provisioner
            .provision(second_run)
            .expect("second run provisions a separate worker");

        assert_ne!(first.worker_generation(), second.worker_generation());
        assert!(first.can_accept_run(first_run));
        assert!(!first.can_accept_run(second_run));
        assert!(second.can_accept_run(second_run));
        assert!(!second.can_accept_run(first_run));

        assert_eq!(
            provisioner.provision(second_run),
            Err(CloudAutoScrubExecutionError::ReusedRunId)
        );
        assert!(CloudAutoScrubRunId::new([0; 32]).is_none());
    }
}
