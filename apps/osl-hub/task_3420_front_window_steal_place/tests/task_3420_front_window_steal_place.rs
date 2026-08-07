const TASK_3420: &str = include_str!("../../../../scripts/qa/task3420-front-window-steal-place.ps1");

#[derive(Clone, Debug)]
struct RunRecord {
    label: &'static str,
    text: &'static str,
    exact_placement: bool,
    refusal_name: Option<&'static str>,
    wrong_place: bool,
}

#[derive(Debug)]
struct Task3420Summary {
    control_first_text: &'static str,
    control_first_exact: bool,
    steal_run_count: usize,
    exact_steal_count: usize,
    named_steal_refusal_count: usize,
    wrong_place_count: usize,
    final_control_text: &'static str,
    final_control_exact: bool,
}

fn summarize(records: &[RunRecord]) -> Task3420Summary {
    let first = records
        .iter()
        .find(|record| record.label == "control-first")
        .expect("first control run is recorded");
    let final_control = records
        .iter()
        .find(|record| record.label == "control-final")
        .expect("final control run is recorded");
    let steal_runs = records
        .iter()
        .filter(|record| record.label == "steal")
        .collect::<Vec<_>>();
    let exact_steal_count = steal_runs
        .iter()
        .filter(|record| record.exact_placement)
        .count();
    let named_steal_refusal_count = steal_runs
        .iter()
        .filter(|record| !record.exact_placement && record.refusal_name.is_some())
        .count();
    let wrong_place_count = records.iter().filter(|record| record.wrong_place).count();

    Task3420Summary {
        control_first_text: first.text,
        control_first_exact: first.exact_placement,
        steal_run_count: steal_runs.len(),
        exact_steal_count,
        named_steal_refusal_count,
        wrong_place_count,
        final_control_text: final_control.text,
        final_control_exact: final_control.exact_placement,
    }
}

fn task_3420_passes(summary: &Task3420Summary) -> bool {
    summary.control_first_text == "FOCUS-3420"
        && summary.control_first_exact
        && summary.steal_run_count == 10
        && summary.exact_steal_count >= 1
        && summary.exact_steal_count + summary.named_steal_refusal_count == 10
        && summary.wrong_place_count == 0
        && summary.final_control_text == "FOCUS-3420-AGAIN"
        && summary.final_control_exact
}

fn green_records() -> Vec<RunRecord> {
    let mut records = vec![RunRecord {
        label: "control-first",
        text: "FOCUS-3420",
        exact_placement: true,
        refusal_name: None,
        wrong_place: false,
    }];
    records.push(RunRecord {
        label: "steal",
        text: "FOCUS-3420-RUN-1",
        exact_placement: true,
        refusal_name: None,
        wrong_place: false,
    });
    for index in 2..=10 {
        let refusal_name = match index % 4 {
            0 => "target-front-stolen",
            1 => "composer-focus-stolen",
            2 => "initial-front-stolen",
            _ => "composer-click-obscured",
        };
        records.push(RunRecord {
            label: "steal",
            text: "FOCUS-3420-RUN",
            exact_placement: false,
            refusal_name: Some(refusal_name),
            wrong_place: false,
        });
    }
    records.push(RunRecord {
        label: "control-final",
        text: "FOCUS-3420-AGAIN",
        exact_placement: true,
        refusal_name: None,
        wrong_place: false,
    });
    records
}

#[test]
fn task_3420_runner_states_the_live_sequence_and_records_every_run() {
    assert!(TASK_3420.contains("[int]$Attempts = 10"));
    assert!(TASK_3420.contains("[string]$ControlText = \"FOCUS-3420\""));
    assert!(TASK_3420.contains("[string]$FinalControlText = \"FOCUS-3420-AGAIN\""));
    assert!(TASK_3420.contains("[int]$StealRepeatCount = 30"));
    assert!(TASK_3420.contains("for ($i = 1; $i -le $Attempts; $i++)"));
    assert!(TASK_3420.contains("Add-RunRecord -Label 'steal'"));
    assert!(TASK_3420.contains("stealRepeatCount"));
    assert!(TASK_3420.contains("records = @($records)"));
    assert!(TASK_3420.contains("task3420_front_window_stealing_runs=$($stealRuns.Count)"));
}

#[test]
fn task_3420_accepts_one_exact_placement_and_named_refusals_only() {
    let records = green_records();
    let summary = summarize(&records);

    eprintln!(
        "task3420 control_first={} exact={} steal_runs={} exact_steal_placements={} named_refusals={} wrong_place_runs={} final_control={} final_exact={}",
        summary.control_first_text,
        summary.control_first_exact,
        summary.steal_run_count,
        summary.exact_steal_count,
        summary.named_steal_refusal_count,
        summary.wrong_place_count,
        summary.final_control_text,
        summary.final_control_exact
    );

    assert!(task_3420_passes(&summary));
    assert_eq!(summary.steal_run_count, 10);
    assert_eq!(summary.exact_steal_count, 1);
    assert_eq!(summary.named_steal_refusal_count, 9);
    assert_eq!(summary.wrong_place_count, 0);
}

#[test]
fn task_3420_fails_when_the_placing_job_is_stubbed_to_do_nothing() {
    let mut records = green_records();
    for record in &mut records {
        record.exact_placement = false;
        record.refusal_name = Some("missing-exact-placement-proof");
    }
    let summary = summarize(&records);

    eprintln!(
        "task3420_noop_stub control_first_exact={} exact_steal_placements={} final_control_exact={} check_passed={}",
        summary.control_first_exact,
        summary.exact_steal_count,
        summary.final_control_exact,
        task_3420_passes(&summary)
    );

    assert!(!task_3420_passes(&summary));
    assert_eq!(summary.exact_steal_count, 0);
}

#[test]
fn task_3420_fails_if_any_run_types_in_the_stealing_window() {
    let mut records = green_records();
    records
        .iter_mut()
        .find(|record| record.label == "steal")
        .expect("steal run exists")
        .wrong_place = true;
    let summary = summarize(&records);

    assert_eq!(summary.wrong_place_count, 1);
    assert!(!task_3420_passes(&summary));
}
