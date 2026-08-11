use std::process::ExitCode;

fn main() -> ExitCode {
    match shipping_transition_guard::task_5302_matrix::run_matrix() {
        Ok(report) => {
            for row in &report.rows {
                println!(
                    "TASK5302_ROW gate={} name={} external_ids={} stale_signer={} current_signer={} before_epoch={} after_epoch={} before_grant={} after_grant={} hostile_actions={} hostile_state_changes={} control_state_changes={} verifier_calls={}",
                    row.gate,
                    row.name,
                    row.external_ids.join(","),
                    row.stale_signer,
                    row.current_signer,
                    row.before_epoch,
                    row.after_epoch,
                    row.before_grant,
                    row.after_grant,
                    row.hostile_actions,
                    row.hostile_state_changes,
                    row.control_state_changes,
                    row.verifier_calls,
                );
            }
            println!(
                "TASK5302_MATRIX rows={} gates={} hostile_actions={} hostile_state_changes={} current_controls={} verifier_calls={} final_state_total={} final_state_digest={} finish_line=CHECKED_OFF",
                report.rows.len(),
                report
                    .rows
                    .iter()
                    .map(|row| row.gate.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                report.hostile_actions,
                report.hostile_state_changes,
                report.control_state_changes,
                report.verifier_calls,
                report.final_state.total(),
                report.final_state.digest_hex(),
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
