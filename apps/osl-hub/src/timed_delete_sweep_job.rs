//! The repeating job that sweeps timed-delete records whose time has passed.
//!
//! This module is a *dispatcher* and nothing else. It wakes on a fixed
//! interval, reads the timed-delete records TASK 3306/3307 persisted, keeps the
//! ones whose deadline has not arrived, and hands each due record to the shared
//! cleaner registered for that record's app.
//!
//! It deliberately contains **no deleting code of its own**: no filesystem
//! removal, no cache shred, no ledger rewrite, no per-app delete action. Every
//! removal happens behind one of two seams:
//!
//! * [`SharedAppCleaner`] — one shared cleaner per app, which removes the
//!   message on the service. TASK 3309 fills these in with the very same
//!   per-app cleaners Scrub uses; nothing here may grow a second copy.
//! * [`TimedDeleteWorkSource`] — the record store, which hands over the records
//!   and retires the ones this job reported swept.
//! * [`ProtectedPartStore`] — the OSL service, which holds the protected part
//!   of a protected message and takes it away again (TASK 3323).
//!
//! The one rule that lives here, and only here, is the due rule: a record is
//! swept when `delete_at_unix_seconds <= now`, and never before.
//!
//! TASK 3323 adds the second half of a delete. A protected message is two
//! things: cover words sitting in the app, and the protected part sitting on
//! the OSL service. Taking away only the cover words is half a delete, so a due
//! protected record is only swept once both halves are gone, and the protected
//! part goes first: if the service will not let go of it, the cover words stay
//! where they are and the record stays in the store to be tried again.

use std::collections::BTreeSet;
use std::fmt;

/// Smallest wake interval the job accepts, in seconds.
pub const MIN_WAKE_EVERY_SECONDS: u32 = 1;
/// Largest wake interval the job accepts, in seconds (one day).
pub const MAX_WAKE_EVERY_SECONDS: u32 = 86_400;
/// Upper bound on records considered in one wake, so a huge store cannot pin
/// the job in a single pass.
pub const MAX_RECORDS_PER_WAKE: usize = 4_096;

/// The word a record carries when the message has a protected part on the OSL
/// service, as written by TASK 3306/3307.
pub const PROTECTED: &str = "protected";

/// The one carrier whose delivered cover must never be deleted by an OSL timer.
///
/// Email can only make a protected pointer stop resolving.  Its delivered cover
/// remains in the mailbox, even when a provider-side delete action would have
/// failed.  Keeping this rule in the sweep, immediately after the service has
/// removed the protected part, makes the service-side destruction independent
/// of any carrier result.
pub const POINTER_ONLY_EMAIL_APP_ID: &str = "email";

/// A timed-delete record for ordinary email is outside the product's scope.
/// `email_timer_contract` refuses creating one; this second guard prevents a
/// malformed persisted row from becoming a carrier-delete path later.
pub const ORDINARY_EMAIL_TIMER_REFUSED: &str = "ordinary_email_timer_refused";

/// The name a fetch of an already-taken-away protected part is refused by.
///
/// The pointer in the cover words leads here, and once the protected part is
/// gone the service must say so by this name rather than by returning nothing.
pub const PROTECTED_PART_GONE: &str = "osl_protected_part_gone";

/// The refusal code for a due record whose protected part the service would not
/// take away.
pub const PROTECTED_PART_NOT_REMOVED: &str = "protected_part_not_removed";

/// The refusal code for a service that reported the protected part taken away
/// while still holding one.
pub const PROTECTED_PART_STILL_STORED: &str = "protected_part_still_stored";

/// One timed-delete record, in the shape this job needs to route it.
///
/// The field names mirror the persisted record from TASK 3306/3307 so the
/// adapter that reads the sealed ledger is a straight field-for-field move.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DueTimedDeleteRecord {
    /// Which app the message lives in, e.g. `discord`.
    pub app_id: String,
    /// Which conversation inside that app.
    pub conversation_id: String,
    /// Enough to find the exact message again on the service.
    pub message_locator: String,
    /// When the message was sent.
    pub sent_at_unix_seconds: i64,
    /// When the message must go.
    pub delete_at_unix_seconds: i64,
    /// `protected` or `ordinary`, carried through untouched.
    pub protection: String,
}

impl DueTimedDeleteRecord {
    /// A stable identity for one record: app, conversation and locator.
    pub fn identity(&self) -> String {
        format!(
            "{}/{}/{}",
            self.app_id, self.conversation_id, self.message_locator
        )
    }

    /// Whether this message has a protected part held on the OSL service.
    ///
    /// The word is carried through from the persisted record untouched, so a
    /// record that says anything else is treated as an ordinary message with
    /// nothing stored for it.
    pub fn is_protected(&self) -> bool {
        self.protection == PROTECTED
    }
}

/// The shared cleaner for one app.
///
/// This job never implements this trait. Each app's own cleaner does, and TASK
/// 3309 points those at the same per-app delete action Scrub already uses.
pub trait SharedAppCleaner {
    /// The app this cleaner is the cleaner for, e.g. `discord`.
    fn app_id(&self) -> &str;

    /// Hand one due record to the shared cleaner.
    ///
    /// Every actual removal happens on the other side of this call.
    fn clean_due_message(&mut self, record: &DueTimedDeleteRecord) -> Result<(), String>;
}

/// What the OSL service did when it was asked to take away one protected part.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProtectedPartRemoval {
    /// The name the service knows the stored protected part by.
    ///
    /// The job never works this name out for itself: the service owns the
    /// mapping from a message to the item it holds for it.
    pub part_name: String,
    /// How many protected items the service held for this message before.
    pub stored_before: usize,
    /// How many it holds for this message now. Anything but 0 is half a delete.
    pub stored_after: usize,
}

/// Why the OSL service refused to hand over a protected part.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedPartFetchRefusal {
    /// A short machine-readable name for the refusal.
    pub code: &'static str,
    /// The protected part that was asked for, by the service's own name for it.
    pub part_name: String,
    /// The human-readable refusal.
    pub reason: String,
}

impl fmt::Display for ProtectedPartFetchRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.reason)
    }
}

/// The OSL service, as far as the protected part of a message is concerned.
///
/// This job never implements this trait either. The service does, and every
/// item it holds leaves on the other side of `remove_protected_part`.
///
/// `fetch_protected_part` is on the same trait on purpose: the pointer carried
/// in the cover words is a fetch, so a service that can take the item away but
/// cannot then refuse the fetch by name has not finished the delete. TASK 3325
/// checks that refusal from the second machine.
pub trait ProtectedPartStore {
    /// How many protected items the service holds for this message right now.
    fn stored_part_count(&self, record: &DueTimedDeleteRecord) -> Result<usize, String>;

    /// Take away the protected part the service holds for this message.
    ///
    /// Asking twice is not an error: a message with nothing stored for it
    /// reports 0 before and 0 after, so a retried sweep does not get stuck.
    fn remove_protected_part(
        &mut self,
        record: &DueTimedDeleteRecord,
    ) -> Result<ProtectedPartRemoval, String>;

    /// Read the protected part back, the way the pointer in the cover words
    /// does. Once it is gone this must be refused by name.
    fn fetch_protected_part(
        &self,
        record: &DueTimedDeleteRecord,
    ) -> Result<Vec<u8>, ProtectedPartFetchRefusal>;
}

/// Where the job reads records from, and where it reports swept ones back to.
///
/// The store — not this job — decides how a retired record leaves the ledger.
pub trait TimedDeleteWorkSource {
    /// Every timed-delete record currently held, due or not.
    ///
    /// The job, not the source, applies the due rule; a source that pre-filtered
    /// would hide the rule this job exists to enforce.
    fn all_records(&self) -> Result<Vec<DueTimedDeleteRecord>, String>;

    /// Report that the shared cleaner accepted this record, so the store may
    /// stop offering it.
    fn retire_swept_record(&mut self, record: &DueTimedDeleteRecord) -> Result<(), String>;
}

/// The shared cleaners this job may route to, one per app.
#[derive(Default)]
pub struct SharedAppCleaners {
    entries: Vec<Box<dyn SharedAppCleaner>>,
}

impl fmt::Debug for SharedAppCleaners {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedAppCleaners")
            .field("app_ids", &self.app_ids())
            .finish()
    }
}

impl SharedAppCleaners {
    /// An empty set of cleaners.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the one shared cleaner for an app.
    ///
    /// A second cleaner for the same app is refused: "exactly one delete action
    /// per app" is the rule TASK 3309 is checked against, and a job that
    /// quietly held two would make that rule unmeasurable.
    pub fn register(&mut self, cleaner: Box<dyn SharedAppCleaner>) -> Result<(), String> {
        let app_id = cleaner.app_id().to_owned();
        if app_id.is_empty() {
            return Err("OSL timed-delete sweep cleaner is missing its app".to_owned());
        }
        if self.entries.iter().any(|entry| entry.app_id() == app_id) {
            return Err(format!(
                "OSL timed-delete sweep already has a cleaner for app {app_id}"
            ));
        }
        self.entries.push(cleaner);
        Ok(())
    }

    /// The apps that have a registered cleaner, in registration order.
    pub fn app_ids(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.app_id().to_owned())
            .collect()
    }

    /// How many cleaners are registered.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no cleaner is registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn cleaner_for(&mut self, app_id: &str) -> Option<&mut Box<dyn SharedAppCleaner>> {
        self.entries
            .iter_mut()
            .find(|entry| entry.app_id() == app_id)
    }
}

/// Why one due record was not swept.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SweepRefusal {
    /// The record that was not swept.
    pub identity: String,
    /// A short machine-readable reason.
    pub code: &'static str,
    /// The human-readable reason, carried from the seam that refused.
    pub reason: String,
}

/// What one wake of the job did.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TimedDeleteSweepPass {
    /// The wall-clock second this pass judged records against.
    pub swept_at_unix_seconds: i64,
    /// Records handed to a shared cleaner and accepted, by identity.
    pub swept: Vec<String>,
    /// Records whose deadline has not arrived, by identity.
    pub retained: Vec<String>,
    /// Due records that were not swept, with the reason.
    pub refused: Vec<SweepRefusal>,
    /// Records past `MAX_RECORDS_PER_WAKE` that this wake did not look at.
    pub deferred: usize,
    /// The protected parts this pass had the OSL service take away, in order.
    pub protected_parts_removed: Vec<ProtectedPartRemoval>,
}

impl TimedDeleteSweepPass {
    /// How many records this pass handed to a cleaner and had accepted.
    pub fn swept_count(&self) -> usize {
        self.swept.len()
    }

    /// How many records this pass left in place because they are not yet due.
    pub fn retained_count(&self) -> usize {
        self.retained.len()
    }

    /// How many protected parts this pass had the service take away.
    pub fn protected_part_count(&self) -> usize {
        self.protected_parts_removed.len()
    }
}

/// What a repeating run of the job did across all its wakes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TimedDeleteSweepRun {
    /// Every wake, in order.
    pub passes: Vec<TimedDeleteSweepPass>,
}

impl TimedDeleteSweepRun {
    /// How many wakes happened.
    pub fn wake_count(&self) -> usize {
        self.passes.len()
    }

    /// Every record swept across the whole run, in order, without repeats.
    pub fn swept(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut swept = Vec::new();
        for pass in &self.passes {
            for identity in &pass.swept {
                if seen.insert(identity.clone()) {
                    swept.push(identity.clone());
                }
            }
        }
        swept
    }

    /// How many records were swept across the whole run.
    pub fn swept_count(&self) -> usize {
        self.swept().len()
    }

    /// Every protected part the run had the service take away, in order.
    pub fn protected_parts_removed(&self) -> Vec<ProtectedPartRemoval> {
        let mut removals = Vec::new();
        for pass in &self.passes {
            for removal in &pass.protected_parts_removed {
                removals.push(removal.clone());
            }
        }
        removals
    }

    /// How many protected parts the run had the service take away.
    pub fn protected_part_count(&self) -> usize {
        self.protected_parts_removed().len()
    }
}

/// Whether the repeating job should wake again.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SweepWaitOutcome {
    /// Wake and sweep again.
    Wake,
    /// Stop the job.
    Stop,
}

/// The clock the repeating job sleeps on.
///
/// A test clock scripts the wall-clock seconds; the real one sleeps.
pub trait SweepClock {
    /// The current wall-clock second.
    fn now_unix_seconds(&self) -> i64;

    /// Wait until `wake_at_unix_seconds`, or say to stop.
    fn wait_until(&mut self, wake_at_unix_seconds: i64) -> SweepWaitOutcome;
}

/// The repeating job that sweeps what is due.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimedDeleteSweepJob {
    wake_every_seconds: u32,
}

impl TimedDeleteSweepJob {
    /// A job that wakes every `wake_every_seconds` seconds.
    pub fn new(wake_every_seconds: u32) -> Result<Self, String> {
        if !(MIN_WAKE_EVERY_SECONDS..=MAX_WAKE_EVERY_SECONDS).contains(&wake_every_seconds) {
            return Err(format!(
                "OSL timed-delete sweep wake interval must be {MIN_WAKE_EVERY_SECONDS}..={MAX_WAKE_EVERY_SECONDS} seconds"
            ));
        }
        Ok(Self { wake_every_seconds })
    }

    /// How often this job wakes.
    pub fn wake_every_seconds(&self) -> u32 {
        self.wake_every_seconds
    }

    /// When this job wakes next, given it just woke at `now`.
    pub fn next_wake_at(&self, now: i64) -> i64 {
        now.saturating_add(i64::from(self.wake_every_seconds))
    }

    /// The due rule, and the only place it lives.
    ///
    /// A record is due once the second it named has arrived, and not one second
    /// earlier. TASK 3308b breaks exactly this to prove the check can fail.
    pub fn is_due(&self, now: i64, record: &DueTimedDeleteRecord) -> bool {
        record.delete_at_unix_seconds <= now
    }

    /// Ask the OSL service to take away the protected part of one due message.
    ///
    /// The job does not decide what the item is called or where it lives; it
    /// only checks the one thing it is entitled to check, that nothing is left
    /// stored for that message afterwards.
    fn take_protected_part(
        &self,
        record: &DueTimedDeleteRecord,
        parts: &mut dyn ProtectedPartStore,
    ) -> Result<ProtectedPartRemoval, SweepRefusal> {
        let identity = record.identity();
        let removal = parts
            .remove_protected_part(record)
            .map_err(|reason| SweepRefusal {
                identity: identity.clone(),
                code: PROTECTED_PART_NOT_REMOVED,
                reason,
            })?;
        if removal.stored_after != 0 {
            return Err(SweepRefusal {
                identity,
                code: PROTECTED_PART_STILL_STORED,
                reason: format!(
                    "OSL service still holds {} protected item(s) named {} for this message",
                    removal.stored_after, removal.part_name
                ),
            });
        }
        Ok(removal)
    }

    /// One wake: read the records, keep the ones that are not due, and for every
    /// due one take the protected part off the OSL service and hand the message
    /// to the shared cleaner for its app.
    ///
    /// The protected part goes first. If the service will not let go of it the
    /// cover words are left alone and the record stays, because a message whose
    /// visible half is gone and whose protected half is still stored cannot be
    /// tried again cleanly.
    pub fn wake(
        &self,
        now: i64,
        work: &mut dyn TimedDeleteWorkSource,
        cleaners: &mut SharedAppCleaners,
        parts: &mut dyn ProtectedPartStore,
    ) -> Result<TimedDeleteSweepPass, String> {
        let records = work.all_records()?;
        let mut pass = TimedDeleteSweepPass {
            swept_at_unix_seconds: now,
            ..TimedDeleteSweepPass::default()
        };

        if records.len() > MAX_RECORDS_PER_WAKE {
            pass.deferred = records.len() - MAX_RECORDS_PER_WAKE;
        }

        for record in records.iter().take(MAX_RECORDS_PER_WAKE) {
            let identity = record.identity();

            if !self.is_due(now, record) {
                pass.retained.push(identity);
                continue;
            }

            // Pointer-only email has no carrier deletion action at all.  Every
            // other app must still prove that its one shared cleaner exists
            // before the protected half can be taken away.
            if record.app_id != POINTER_ONLY_EMAIL_APP_ID
                && cleaners.cleaner_for(&record.app_id).is_none()
            {
                pass.refused.push(SweepRefusal {
                    identity,
                    code: "no_cleaner_for_app",
                    reason: format!(
                        "OSL timed-delete sweep has no shared cleaner for app {}",
                        record.app_id
                    ),
                });
                continue;
            }

            // The protected half first, so a service that will not let go never
            // leaves a message with its cover words gone and its private words
            // still stored.
            if record.is_protected() {
                match self.take_protected_part(record, parts) {
                    Ok(removal) => pass.protected_parts_removed.push(removal),
                    Err(refusal) => {
                        pass.refused.push(refusal);
                        continue;
                    }
                }
            }

            // A protected email timer is pointer-only.  The protected part is
            // already gone above, so the pointer now refuses; deliberately do
            // not ask any carrier adapter to delete the delivered cover.  This
            // also means a forced carrier failure cannot strand the private
            // part on the OSL service.  Retiring the record is safe because
            // the only promised deletion has completed.
            if record.app_id == POINTER_ONLY_EMAIL_APP_ID {
                if record.is_protected() {
                    work.retire_swept_record(record)?;
                    pass.swept.push(identity);
                } else {
                    pass.refused.push(SweepRefusal {
                        identity,
                        code: ORDINARY_EMAIL_TIMER_REFUSED,
                        reason: "ordinary email is refused: protected email is required; records=0"
                            .to_owned(),
                    });
                }
                continue;
            }

            // Refused just above if it were missing, so this is the same
            // cleaner, looked up again only because the borrow had to end for
            // the protected half.
            let Some(cleaner) = cleaners.cleaner_for(&record.app_id) else {
                continue;
            };

            match cleaner.clean_due_message(record) {
                Ok(()) => {
                    work.retire_swept_record(record)?;
                    pass.swept.push(identity);
                }
                Err(reason) => pass.refused.push(SweepRefusal {
                    identity,
                    code: "cleaner_refused",
                    reason,
                }),
            }
        }

        Ok(pass)
    }

    /// Run the job repeatedly until the clock says stop.
    ///
    /// This is the whole of the job's repetition: wake, sweep, sleep until the
    /// next wake, repeat.
    pub fn run_repeating(
        &self,
        clock: &mut dyn SweepClock,
        work: &mut dyn TimedDeleteWorkSource,
        cleaners: &mut SharedAppCleaners,
        parts: &mut dyn ProtectedPartStore,
    ) -> Result<TimedDeleteSweepRun, String> {
        let mut run = TimedDeleteSweepRun::default();
        loop {
            let now = clock.now_unix_seconds();
            let pass = self.wake(now, work, cleaners, parts)?;
            run.passes.push(pass);
            if clock.wait_until(self.next_wake_at(now)) == SweepWaitOutcome::Stop {
                return Ok(run);
            }
        }
    }
}

/// The real clock: the machine's wall clock, and a real sleep between wakes.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemSweepClock;

impl SweepClock for SystemSweepClock {
    fn now_unix_seconds(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs() as i64)
            .unwrap_or(0)
    }

    fn wait_until(&mut self, wake_at_unix_seconds: i64) -> SweepWaitOutcome {
        let remaining = wake_at_unix_seconds.saturating_sub(self.now_unix_seconds());
        if remaining > 0 {
            std::thread::sleep(std::time::Duration::from_secs(remaining as u64));
        }
        SweepWaitOutcome::Wake
    }
}
