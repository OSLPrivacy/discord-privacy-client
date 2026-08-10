//! The on-device byte-meter record.
//!
//! TASK 4601 chose anonymous vouchers rather than a server-side per-account
//! ledger. This DTO therefore carries only local arithmetic: a calendar month,
//! a byte count, one closed byte class, and an opaque source id. Strict serde
//! decoding prevents message text, file names, account ids, or other fields
//! from being smuggled into the record.

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::HashSet, fmt};

const SECONDS_PER_DAY: i64 = 86_400;

// Keep the persisted byte-class vocabulary in one canonical definition. The
// enum, serializer, parser, fixtures, and later metering hooks all derive their
// names from this array.
const BYTE_CLASS_NAMES: [&str; 6] = [
    "background connection",
    "messages",
    "attachments",
    "stories and posts",
    "voice",
    "multi-device sync",
];

/// The complete, closed set of traffic classes counted by the on-device meter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeteredByteClass {
    BackgroundConnection,
    Messages,
    Attachments,
    StoriesAndPosts,
    Voice,
    MultiDeviceSync,
}

impl MeteredByteClass {
    /// Every class, in the stable order used by itemised meter displays.
    pub const ALL: [Self; 6] = [
        Self::BackgroundConnection,
        Self::Messages,
        Self::Attachments,
        Self::StoriesAndPosts,
        Self::Voice,
        Self::MultiDeviceSync,
    ];

    /// The stable persisted and displayed name of this class.
    pub const fn name(self) -> &'static str {
        BYTE_CLASS_NAMES[self.index()]
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|byte_class| byte_class.name() == name)
    }

    const fn index(self) -> usize {
        match self {
            Self::BackgroundConnection => 0,
            Self::Messages => 1,
            Self::Attachments => 2,
            Self::StoriesAndPosts => 3,
            Self::Voice => 4,
            Self::MultiDeviceSync => 5,
        }
    }
}

impl Serialize for MeteredByteClass {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for MeteredByteClass {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let name = String::deserialize(deserializer)?;
        Self::from_name(&name).ok_or_else(|| D::Error::unknown_variant(&name, &BYTE_CLASS_NAMES))
    }
}

/// One contribution to the person's on-device monthly byte arithmetic.
///
/// `byte_count` is unsigned, so negative JSON input is rejected by serde. The
/// strict four-field shape is intentional: payload text and identifying file
/// metadata do not belong in usage accounting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeteredByteRecord {
    pub month: String,
    pub byte_count: u64,
    pub byte_class: MeteredByteClass,
    pub source_id: String,
}

impl MeteredByteRecord {
    pub fn new(
        month: impl Into<String>,
        byte_count: u64,
        byte_class: MeteredByteClass,
        source_id: impl Into<String>,
    ) -> Self {
        Self {
            month: month.into(),
            byte_count,
            byte_class,
            source_id: source_id.into(),
        }
    }
}

/// Every shipping byte-producing path covered by ruling A7.
///
/// Voice intentionally is not a variant here: no voice client ships in this
/// release. It remains a required byte class (and therefore a visible zero in
/// totals) so adding voice later cannot silently bypass allowance accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeteredSendPath {
    HeldMessageBytes,
    Attachments,
    StoryAndPostMedia,
    MultiDeviceSyncTraffic,
    BackgroundCoverTick,
    PlainText,
}

const METERED_SEND_PATH_HOOKS: [(MeteredSendPath, Option<MeteredByteClass>); 6] = [
    (
        MeteredSendPath::HeldMessageBytes,
        Some(MeteredByteClass::Messages),
    ),
    (
        MeteredSendPath::Attachments,
        Some(MeteredByteClass::Attachments),
    ),
    (
        MeteredSendPath::StoryAndPostMedia,
        Some(MeteredByteClass::StoriesAndPosts),
    ),
    (
        MeteredSendPath::MultiDeviceSyncTraffic,
        Some(MeteredByteClass::MultiDeviceSync),
    ),
    (
        MeteredSendPath::BackgroundCoverTick,
        Some(MeteredByteClass::BackgroundConnection),
    ),
    (MeteredSendPath::PlainText, Some(MeteredByteClass::Messages)),
];

impl MeteredSendPath {
    pub const ALL: [Self; 6] = [
        Self::HeldMessageBytes,
        Self::Attachments,
        Self::StoryAndPostMedia,
        Self::MultiDeviceSyncTraffic,
        Self::BackgroundCoverTick,
        Self::PlainText,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::HeldMessageBytes => "held message bytes",
            Self::Attachments => "attachments",
            Self::StoryAndPostMedia => "story and post media",
            Self::MultiDeviceSyncTraffic => "multi-device sync traffic",
            Self::BackgroundCoverTick => "constant background cover tick",
            Self::PlainText => "plain text",
        }
    }

    pub fn byte_class(self) -> Option<MeteredByteClass> {
        METERED_SEND_PATH_HOOKS
            .into_iter()
            .find_map(|(path, byte_class)| (path == self).then_some(byte_class))
            .flatten()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissingByteClassHook {
    pub path: MeteredSendPath,
}

impl fmt::Display for MissingByteClassHook {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "metered send path has no byte-class hook: {}",
            self.path.name()
        )
    }
}

impl std::error::Error for MissingByteClassHook {}

/// Production enables all hooks. `without` is a fault-injection seam proving
/// that each class contributes exactly its measured bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeteredByteHooks {
    enabled: [bool; 6],
}

impl MeteredByteHooks {
    pub const fn all() -> Self {
        Self { enabled: [true; 6] }
    }

    pub fn without(mut self, byte_class: MeteredByteClass) -> Self {
        self.enabled[byte_class.index()] = false;
        self
    }

    pub const fn is_enabled(self, byte_class: MeteredByteClass) -> bool {
        self.enabled[byte_class.index()]
    }
}

impl Default for MeteredByteHooks {
    fn default() -> Self {
        Self::all()
    }
}

/// The on-device meter before any allowance top-up arithmetic is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeteredByteMeter {
    hooks: MeteredByteHooks,
    records: Vec<MeteredByteRecord>,
}

impl Default for MeteredByteMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl MeteredByteMeter {
    pub fn new() -> Self {
        Self::with_hooks(MeteredByteHooks::all())
    }

    pub fn with_hooks(hooks: MeteredByteHooks) -> Self {
        Self {
            hooks,
            records: Vec::new(),
        }
    }

    pub fn record_send(
        &mut self,
        month: impl Into<String>,
        path: MeteredSendPath,
        byte_count: u64,
        source_id: impl Into<String>,
    ) -> Result<bool, MissingByteClassHook> {
        let byte_class = path.byte_class().ok_or(MissingByteClassHook { path })?;
        if !self.hooks.is_enabled(byte_class) {
            return Ok(false);
        }
        self.records.push(MeteredByteRecord::new(
            month, byte_count, byte_class, source_id,
        ));
        Ok(true)
    }

    /// Six rows are always returned, including zero-valued absent features.
    pub fn class_totals(&self) -> Vec<(MeteredByteClass, u64)> {
        let mut totals = [0_u64; 6];
        for record in &self.records {
            totals[record.byte_class.index()] = totals[record.byte_class.index()]
                .checked_add(record.byte_count)
                .expect("metered byte total overflow");
        }
        MeteredByteClass::ALL
            .into_iter()
            .map(|byte_class| (byte_class, totals[byte_class.index()]))
            .collect()
    }

    pub fn total_before_top_ups(&self) -> u64 {
        self.class_totals()
            .into_iter()
            .map(|(_, bytes)| bytes)
            .sum()
    }

    pub fn records(&self) -> &[MeteredByteRecord] {
        &self.records
    }
}

/// Timestamped account allowance accounting. This is deliberately separate
/// from [`MeteredByteMeter`], whose caller-supplied month exists only for the
/// task-4604 hook inventory. Production callers cannot accidentally mix an
/// untimed lifetime total with the calendar-windowed allowance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthlyAllowanceMeter {
    hooks: MeteredByteHooks,
    account_reset_clock: AccountResetClock,
    timed_records: Vec<TimedMeteredByteRecord>,
}

impl MonthlyAllowanceMeter {
    /// Build the production meter for an account. The supplied instant is one
    /// of that account's monthly resets; subsequent resets retain its UTC day
    /// and time, clamping only when a calendar month is shorter.
    pub fn new(account_reset_unix_seconds: i64) -> Self {
        Self::with_hooks(account_reset_unix_seconds, MeteredByteHooks::all())
    }

    /// Fault-injection counterpart of [`Self::new`].
    pub fn with_hooks(account_reset_unix_seconds: i64, hooks: MeteredByteHooks) -> Self {
        Self {
            hooks,
            account_reset_clock: AccountResetClock::new(account_reset_unix_seconds),
            timed_records: Vec::new(),
        }
    }

    /// Record transfer bytes at an absolute instant. The month is derived from
    /// the account reset clock and cannot be selected by the caller.
    pub fn record_send_at(
        &mut self,
        unix_seconds: i64,
        path: MeteredSendPath,
        byte_count: u64,
        source_id: impl Into<String>,
    ) -> Result<bool, MeteredByteWindowError> {
        let byte_class = path.byte_class().ok_or(MissingByteClassHook { path })?;
        self.record_timed(unix_seconds, byte_count, byte_class, source_id.into(), 0)
    }

    /// Record one completed message and all of its metered transfer bytes on
    /// one clock tick. Keeping these two facts in one event prevents their
    /// rolling 24-hour figures from acquiring different reset cutoffs.
    pub fn record_message_at(
        &mut self,
        unix_seconds: i64,
        byte_count: u64,
        source_id: impl Into<String>,
    ) -> Result<bool, MeteredByteWindowError> {
        self.record_timed(
            unix_seconds,
            byte_count,
            MeteredByteClass::Messages,
            source_id.into(),
            1,
        )
    }

    fn record_timed(
        &mut self,
        unix_seconds: i64,
        byte_count: u64,
        byte_class: MeteredByteClass,
        source_id: String,
        message_count: u64,
    ) -> Result<bool, MeteredByteWindowError> {
        if !self.hooks.is_enabled(byte_class) {
            return Ok(false);
        }
        let window = self.account_reset_clock.month_window_at(unix_seconds)?;
        let record = MeteredByteRecord::new(window.label(), byte_count, byte_class, source_id);
        self.timed_records.push(TimedMeteredByteRecord {
            unix_seconds,
            record,
            message_count,
        });
        Ok(true)
    }

    /// Read the current allowance month, the preceding read-only receipt, and
    /// the two figures sharing the exact same rolling 24-hour cutoff.
    pub fn usage_at(
        &self,
        unix_seconds: i64,
    ) -> Result<MeteredAllowanceUsage, MeteredByteWindowError> {
        let current_window = self.account_reset_clock.month_window_at(unix_seconds)?;
        let previous_instant = current_window
            .start_unix_seconds
            .checked_sub(1)
            .ok_or(MeteredByteWindowError::TimestampOutOfRange)?;
        let previous_window = self.account_reset_clock.month_window_at(previous_instant)?;

        let current_month = self.receipt_for(current_window)?;
        let previous_month = self.receipt_for(previous_window)?;
        let cutoff = unix_seconds
            .checked_sub(SECONDS_PER_DAY)
            .ok_or(MeteredByteWindowError::TimestampOutOfRange)?;
        let mut transfer_bytes = 0_u64;
        let mut message_count = 0_u64;
        for event in self
            .timed_records
            .iter()
            .filter(|event| event.unix_seconds > cutoff && event.unix_seconds <= unix_seconds)
        {
            transfer_bytes = transfer_bytes
                .checked_add(event.record.byte_count)
                .ok_or(MeteredByteWindowError::CounterOverflow)?;
            message_count = message_count
                .checked_add(event.message_count)
                .ok_or(MeteredByteWindowError::CounterOverflow)?;
        }

        Ok(MeteredAllowanceUsage {
            current_month,
            previous_month,
            rolling_24_hours: RollingDayUsage {
                window_start_exclusive: cutoff,
                window_end_inclusive: unix_seconds,
                transfer_bytes,
                message_count,
            },
        })
    }

    fn receipt_for(
        &self,
        window: CalendarMonthWindow,
    ) -> Result<MonthlyMeterReceipt, MeteredByteWindowError> {
        let mut totals = [0_u64; 6];
        for event in self.timed_records.iter().filter(|event| {
            event.unix_seconds >= window.start_unix_seconds
                && event.unix_seconds < window.end_unix_seconds
        }) {
            let slot = &mut totals[event.record.byte_class.index()];
            *slot = slot
                .checked_add(event.record.byte_count)
                .ok_or(MeteredByteWindowError::CounterOverflow)?;
        }
        let total_bytes = totals.iter().try_fold(0_u64, |total, bytes| {
            total
                .checked_add(*bytes)
                .ok_or(MeteredByteWindowError::CounterOverflow)
        })?;
        Ok(MonthlyMeterReceipt {
            window,
            class_totals: MeteredByteClass::ALL
                .map(|byte_class| (byte_class, totals[byte_class.index()])),
            total_bytes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TimedMeteredByteRecord {
    unix_seconds: i64,
    record: MeteredByteRecord,
    message_count: u64,
}

/// A calendar month selected by an account's reset clock. Fields are exposed
/// through getters so a completed month is a receipt, not a writable bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarMonthWindow {
    start_unix_seconds: i64,
    end_unix_seconds: i64,
    label_year: i64,
    label_month: u8,
}

impl CalendarMonthWindow {
    pub const fn start_unix_seconds(self) -> i64 {
        self.start_unix_seconds
    }

    pub const fn end_unix_seconds(self) -> i64 {
        self.end_unix_seconds
    }

    pub fn label(self) -> String {
        format!("{:04}-{:02}", self.label_year, self.label_month)
    }
}

/// The immutable view of one completed or current allowance month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthlyMeterReceipt {
    window: CalendarMonthWindow,
    class_totals: [(MeteredByteClass, u64); 6],
    total_bytes: u64,
}

impl MonthlyMeterReceipt {
    pub const fn window(&self) -> CalendarMonthWindow {
        self.window
    }

    pub const fn class_totals(&self) -> &[(MeteredByteClass, u64); 6] {
        &self.class_totals
    }

    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
}

/// Transfer and message figures computed from one shared `(now - 24h, now]`
/// interval. There is deliberately no local-date or timezone input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollingDayUsage {
    window_start_exclusive: i64,
    window_end_inclusive: i64,
    transfer_bytes: u64,
    message_count: u64,
}

impl RollingDayUsage {
    pub const fn window_start_exclusive(self) -> i64 {
        self.window_start_exclusive
    }

    pub const fn window_end_inclusive(self) -> i64 {
        self.window_end_inclusive
    }

    pub const fn transfer_bytes(self) -> u64 {
        self.transfer_bytes
    }

    pub const fn message_count(self) -> u64 {
        self.message_count
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeteredAllowanceUsage {
    current_month: MonthlyMeterReceipt,
    previous_month: MonthlyMeterReceipt,
    rolling_24_hours: RollingDayUsage,
}

impl MeteredAllowanceUsage {
    pub const fn current_month(&self) -> &MonthlyMeterReceipt {
        &self.current_month
    }

    pub const fn previous_month(&self) -> &MonthlyMeterReceipt {
        &self.previous_month
    }

    pub const fn rolling_24_hours(&self) -> RollingDayUsage {
        self.rolling_24_hours
    }
}

/// The allowance figures used by the local anonymous-voucher meter.
///
/// These are decimal GB so their rendered values agree with the purchase and
/// settings surfaces (for example, 20.00 GB is 20,000,000,000 bytes).  A
/// voucher is redeemed only by the client that holds it; neither this type nor
/// [`MonthlyAllowanceMeter`] contains an account identifier or any store-side
/// record.
pub const FREE_MONTHLY_ALLOWANCE_BYTES: u64 = 20_000_000_000;
pub const TINY_TEST_TOP_UP_ALLOWANCE_BYTES: u64 = 1_000_000_000;

/// A denomination of anonymous allowance credit.
///
/// The test pack is deliberately a stable identifier: payment work can mint
/// one opaque voucher for it without teaching the byte meter who bought it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TopUpPack {
    TinyTest,
}

impl TopUpPack {
    pub const fn code(self) -> &'static str {
        match self {
            Self::TinyTest => "TINY-TEST",
        }
    }

    pub const fn allowance_bytes(self) -> u64 {
        match self {
            Self::TinyTest => TINY_TEST_TOP_UP_ALLOWANCE_BYTES,
        }
    }
}

/// A local snapshot of this account's current calendar-window allowance.
/// `spent_bytes` is always metered transfer data; it never includes a top-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthlyAllowanceSnapshot {
    cap_bytes: u64,
    spent_bytes: u64,
}

impl MonthlyAllowanceSnapshot {
    pub const fn cap_bytes(self) -> u64 {
        self.cap_bytes
    }

    pub const fn spent_bytes(self) -> u64 {
        self.spent_bytes
    }

    pub const fn remaining_bytes(self) -> u64 {
        self.cap_bytes.saturating_sub(self.spent_bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RedeemedTopUp {
    voucher_id: String,
    window: CalendarMonthWindow,
    allowance_bytes: u64,
}

/// Local allowance arithmetic for one person.  The caller owns one instance
/// per account/device context; that separation is intentional, and does not
/// create an account ledger at the cipher store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthlyAllowanceAccount {
    meter: MonthlyAllowanceMeter,
    redeemed_voucher_ids: HashSet<String>,
    redeemed_top_ups: Vec<RedeemedTopUp>,
}

impl MonthlyAllowanceAccount {
    pub fn new(account_reset_unix_seconds: i64) -> Self {
        Self {
            meter: MonthlyAllowanceMeter::new(account_reset_unix_seconds),
            redeemed_voucher_ids: HashSet::new(),
            redeemed_top_ups: Vec::new(),
        }
    }

    pub fn meter_mut(&mut self) -> &mut MonthlyAllowanceMeter {
        &mut self.meter
    }

    /// Redeem an opaque, one-use voucher into the allowance window containing
    /// `unix_seconds`.  Credit raises only that window's cap; the byte meter is
    /// not written here, so a credit cannot look like transfer spending.
    pub fn redeem_top_up_at(
        &mut self,
        unix_seconds: i64,
        pack: TopUpPack,
        voucher_id: impl Into<String>,
    ) -> Result<MonthlyAllowanceSnapshot, TopUpRedemptionError> {
        let voucher_id = voucher_id.into();
        if self.redeemed_voucher_ids.contains(&voucher_id) {
            return Err(TopUpRedemptionError::AlreadyRedeemed);
        }
        let window = self
            .meter
            .account_reset_clock
            .month_window_at(unix_seconds)
            .map_err(TopUpRedemptionError::Meter)?;
        self.redeemed_voucher_ids.insert(voucher_id.clone());
        self.redeemed_top_ups.push(RedeemedTopUp {
            voucher_id,
            window,
            allowance_bytes: pack.allowance_bytes(),
        });
        self.snapshot_at(unix_seconds)
    }

    pub fn snapshot_at(
        &self,
        unix_seconds: i64,
    ) -> Result<MonthlyAllowanceSnapshot, TopUpRedemptionError> {
        let usage = self
            .meter
            .usage_at(unix_seconds)
            .map_err(TopUpRedemptionError::Meter)?;
        let window = usage.current_month().window();
        let credit_bytes = self
            .redeemed_top_ups
            .iter()
            .filter(|credit| credit.window == window)
            .try_fold(0_u64, |total, credit| {
                total
                    .checked_add(credit.allowance_bytes)
                    .ok_or(TopUpRedemptionError::CounterOverflow)
            })?;
        let cap_bytes = FREE_MONTHLY_ALLOWANCE_BYTES
            .checked_add(credit_bytes)
            .ok_or(TopUpRedemptionError::CounterOverflow)?;
        Ok(MonthlyAllowanceSnapshot {
            cap_bytes,
            spent_bytes: usage.current_month().total_bytes(),
        })
    }

    pub fn redeemed_top_up_count(&self) -> usize {
        self.redeemed_top_ups.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopUpRedemptionError {
    AlreadyRedeemed,
    Meter(MeteredByteWindowError),
    CounterOverflow,
}

impl fmt::Display for TopUpRedemptionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRedeemed => formatter.write_str("already redeemed"),
            Self::Meter(error) => error.fmt(formatter),
            Self::CounterOverflow => formatter.write_str("top-up allowance counter overflow"),
        }
    }
}

impl std::error::Error for TopUpRedemptionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AccountResetClock {
    reset_day: u8,
    reset_second_of_day: i64,
}

impl AccountResetClock {
    fn new(reset_unix_seconds: i64) -> Self {
        let days = reset_unix_seconds.div_euclid(SECONDS_PER_DAY);
        let (_, _, reset_day) = civil_from_days(days);
        Self {
            reset_day,
            reset_second_of_day: reset_unix_seconds.rem_euclid(SECONDS_PER_DAY),
        }
    }

    fn month_window_at(
        self,
        unix_seconds: i64,
    ) -> Result<CalendarMonthWindow, MeteredByteWindowError> {
        let days = unix_seconds.div_euclid(SECONDS_PER_DAY);
        let (year, month, _) = civil_from_days(days);
        let candidate = self.boundary(year, month)?;
        let (start_year, start_month, start) = if unix_seconds >= candidate {
            (year, month, candidate)
        } else {
            let (previous_year, previous_month) = previous_month(year, month);
            (
                previous_year,
                previous_month,
                self.boundary(previous_year, previous_month)?,
            )
        };
        let (end_year, end_month) = next_month(start_year, start_month);
        Ok(CalendarMonthWindow {
            start_unix_seconds: start,
            end_unix_seconds: self.boundary(end_year, end_month)?,
            label_year: start_year,
            label_month: start_month,
        })
    }

    fn boundary(self, year: i64, month: u8) -> Result<i64, MeteredByteWindowError> {
        let day = self.reset_day.min(days_in_month(year, month));
        let days = days_from_civil(year, month, day);
        days.checked_mul(SECONDS_PER_DAY)
            .and_then(|seconds| seconds.checked_add(self.reset_second_of_day))
            .ok_or(MeteredByteWindowError::TimestampOutOfRange)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeteredByteWindowError {
    MissingByteClassHook(MissingByteClassHook),
    TimestampOutOfRange,
    CounterOverflow,
}

impl fmt::Display for MeteredByteWindowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingByteClassHook(error) => error.fmt(formatter),
            Self::TimestampOutOfRange => formatter.write_str("meter timestamp is out of range"),
            Self::CounterOverflow => formatter.write_str("meter counter overflow"),
        }
    }
}

impl std::error::Error for MeteredByteWindowError {}

impl From<MissingByteClassHook> for MeteredByteWindowError {
    fn from(error: MissingByteClassHook) -> Self {
        Self::MissingByteClassHook(error)
    }
}

fn previous_month(year: i64, month: u8) -> (i64, u8) {
    if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

fn next_month(year: i64, month: u8) -> (i64, u8) {
    if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

fn days_in_month(year: i64, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.rem_euclid(4) == 0
            && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0) =>
        {
            29
        }
        2 => 28,
        _ => unreachable!("calendar month is always 1 through 12"),
    }
}

// Howard Hinnant's proleptic-Gregorian civil-date conversion. Both helpers use
// Euclidean division, so reset arithmetic stays defined before Unix epoch too.
fn days_from_civil(year: i64, month: u8, day: u8) -> i64 {
    let adjusted_year = year - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u8, u8) {
    let shifted = days_since_epoch + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month as u8, day as u8)
}

pub fn unhooked_metered_send_path_count() -> usize {
    MeteredSendPath::ALL
        .into_iter()
        .filter(|path| path.byte_class().is_none())
        .count()
}
