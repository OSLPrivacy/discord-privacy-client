-- TASK 6578: unattended retention recovery and terminal owner reporting.
--
-- Ordinary provider failures remain retryable forever.  Only three repeated
-- observations of the same explicitly unrecoverable provider condition may
-- make a claim terminal.  The report state is stored beside the durable claim
-- so scheduler, Worker, service, and machine restarts cannot lose it.

ALTER TABLE attachment_sweep_claims
  ADD COLUMN cleanup_policy TEXT NOT NULL DEFAULT 'attachment_retention';
ALTER TABLE attachment_sweep_claims
  ADD COLUMN unrecoverable_reason TEXT;
ALTER TABLE attachment_sweep_claims
  ADD COLUMN unrecoverable_count INTEGER NOT NULL DEFAULT 0
    CHECK(unrecoverable_count >= 0 AND unrecoverable_count <= 3);
ALTER TABLE attachment_sweep_claims ADD COLUMN terminal_at INTEGER;
ALTER TABLE attachment_sweep_claims
  ADD COLUMN report_status TEXT NOT NULL DEFAULT 'none'
    CHECK(report_status IN ('none', 'pending', 'delivered'));
ALTER TABLE attachment_sweep_claims
  ADD COLUMN report_attempts INTEGER NOT NULL DEFAULT 0
    CHECK(report_attempts >= 0);
ALTER TABLE attachment_sweep_claims
  ADD COLUMN report_next_attempt_at INTEGER NOT NULL DEFAULT 0
    CHECK(report_next_attempt_at >= 0);
ALTER TABLE attachment_sweep_claims ADD COLUMN report_delivered_at INTEGER;

CREATE INDEX idx_attachment_sweep_terminal_report
  ON attachment_sweep_claims(report_status, report_next_attempt_at, terminal_at);
