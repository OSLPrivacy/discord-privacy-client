//! Bounded Messenger delivery state used by the protected receive boundary.
//!
//! A carrier cover is not a private message on its own.  The receive job only
//! makes a message openable when the cover and its paired private words arrive
//! together for the intended account.

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessengerCoverEnvelope {
    pub sender_account: String,
    pub receiver_account: String,
    pub marked_cover: String,
    pub private_words: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MessengerReceiverFailure {
    CoverArrivedOnItsOwn {
        sender_account: String,
        receiver_account: String,
        marked_cover: String,
    },
}

/// Exact private row content emitted by the live Messenger receiving job.
///
/// Eye-state callers deliberately consume this record instead of reopening the
/// carrier envelope.  That keeps the protected row tied to what the receiver
/// actually accepted and recorded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessengerReceivingJobOutput {
    pub sender_account: String,
    pub receiver_account: String,
    pub marked_cover: String,
    pub protected_text: String,
}

#[derive(Default)]
pub struct MessengerTestAccount {
    sent_marked_covers: Vec<MessengerCoverEnvelope>,
    received_private_messages: Vec<MessengerCoverEnvelope>,
    receiving_job_outputs: Vec<MessengerReceivingJobOutput>,
    receiver_failures: Vec<MessengerReceiverFailure>,
}

impl MessengerTestAccount {
    pub fn private_messages_read(&self) -> usize {
        self.received_private_messages.len()
    }

    pub fn sent_marked_cover_count(&self) -> usize {
        self.sent_marked_covers.len()
    }

    pub fn receiver_failures(&self) -> &[MessengerReceiverFailure] {
        &self.receiver_failures
    }

    pub fn receiving_job_outputs(&self) -> &[MessengerReceivingJobOutput] {
        &self.receiving_job_outputs
    }

    pub fn receiving_job_output_for_marked_cover(
        &self,
        marked_cover: &str,
    ) -> Result<&MessengerReceivingJobOutput, String> {
        let mut matches = self
            .receiving_job_outputs
            .iter()
            .filter(|output| output.marked_cover.as_bytes() == marked_cover.as_bytes());
        let output = matches
            .next()
            .ok_or_else(|| "Messenger receiving job has no output for marked row".to_owned())?;
        if matches.next().is_some() {
            return Err("Messenger receiving job output is ambiguous for marked row".to_owned());
        }
        Ok(output)
    }

    pub fn open_next_marked_private_message(&self) -> Result<(&str, &str), String> {
        let received = self
            .received_private_messages
            .last()
            .ok_or_else(|| "second account has no private Messenger message to open".to_owned())?;
        let private_words = received.private_words.as_deref().ok_or_else(|| {
            "received Messenger cover has no private words; it must be a recorded failure"
                .to_owned()
        })?;
        Ok((&received.marked_cover, private_words))
    }
}

#[derive(Default)]
pub struct MessengerTestMachine {
    pub sender: MessengerTestAccount,
    pub receiver: MessengerTestAccount,
    pending_for_receiver: Vec<MessengerCoverEnvelope>,
}

impl MessengerTestMachine {
    pub fn send_marked_cover(
        &mut self,
        sender_account: &str,
        receiver_account: &str,
        marked_cover: &str,
        private_words: Option<&str>,
    ) {
        let envelope = MessengerCoverEnvelope {
            sender_account: sender_account.to_owned(),
            receiver_account: receiver_account.to_owned(),
            marked_cover: marked_cover.to_owned(),
            private_words: private_words.map(str::to_owned),
        };
        self.sender.sent_marked_covers.push(envelope.clone());
        self.pending_for_receiver.push(envelope);
    }
}

pub trait MessengerReceivingJob {
    fn receive_pending(&mut self, machine: &mut MessengerTestMachine) -> usize;
}

/// Receiver implementation used in the normal Messenger job path.
pub struct LiveMessengerReceivingJob;

impl MessengerReceivingJob for LiveMessengerReceivingJob {
    fn receive_pending(&mut self, machine: &mut MessengerTestMachine) -> usize {
        let pending = std::mem::take(&mut machine.pending_for_receiver);
        let mut private_messages_received = 0;
        for envelope in pending {
            if envelope.sender_account == envelope.receiver_account
                || envelope.private_words.is_none()
            {
                machine.receiver.receiver_failures.push(
                    MessengerReceiverFailure::CoverArrivedOnItsOwn {
                        sender_account: envelope.sender_account,
                        receiver_account: envelope.receiver_account,
                        marked_cover: envelope.marked_cover,
                    },
                );
                continue;
            }
            let protected_text = envelope
                .private_words
                .as_ref()
                .expect("checked paired Messenger private words")
                .clone();
            machine
                .receiver
                .receiving_job_outputs
                .push(MessengerReceivingJobOutput {
                    sender_account: envelope.sender_account.clone(),
                    receiver_account: envelope.receiver_account.clone(),
                    marked_cover: envelope.marked_cover.clone(),
                    protected_text,
                });
            machine.receiver.received_private_messages.push(envelope);
            private_messages_received += 1;
        }
        private_messages_received
    }
}

/// Explicit no-op seam for an end-to-end receiving-job break check.
pub struct NoopMessengerReceivingJob;

impl MessengerReceivingJob for NoopMessengerReceivingJob {
    fn receive_pending(&mut self, _machine: &mut MessengerTestMachine) -> usize {
        0
    }
}
