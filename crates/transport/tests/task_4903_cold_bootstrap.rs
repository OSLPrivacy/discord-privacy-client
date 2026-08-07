use transport::tor::ArtiProxyConfig;

const TOR_4903_STREAM: &str = include_str!("fixtures/task_4903_cold_bootstrap.events");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppBootstrapState {
    Connecting,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug)]
struct BootstrapEvent {
    second: u64,
    percent: u8,
}

#[derive(Clone, Copy, Debug)]
struct AppObservation {
    second: u64,
    state: AppBootstrapState,
}

#[test]
fn cold_tor_bootstrap_progress_until_second_75_must_not_fail_at_second_45() {
    let events = parse_bootstrap_stream(TOR_4903_STREAM);
    let progress_events = events.iter().filter(|event| event.percent < 100).count();
    let ready_second = events
        .iter()
        .find(|event| event.percent == 100)
        .map(|event| event.second)
        .expect("fixture stream must reach ready");

    assert!(
        progress_events >= 8,
        "fixture must prove real cold progress before ready; got {progress_events}"
    );
    assert_eq!(
        ready_second, 75,
        "fixture must reach ready at logical second 75"
    );

    let observations = current_app_observations(&events, ArtiProxyConfig::new("arti"));
    if let Some(failed) = observations.iter().find(|observation| {
        observation.state == AppBootstrapState::Failed && observation.second < 60
    }) {
        println!(
            "TOR-4903-RED state=failed second={} progress_events={} ready_second={} observations={:?}",
            failed.second, progress_events, ready_second, observations
        );
        panic!("TOR-4903-RED expected the app to keep reporting connecting until second 75 ready");
    }

    assert!(
        observations.iter().all(|observation| {
            observation.state == AppBootstrapState::Connecting
                || observation.state == AppBootstrapState::Ready
        }),
        "cold bootstrap must stay connecting until ready: {observations:?}"
    );
}

fn current_app_observations(
    events: &[BootstrapEvent],
    config: ArtiProxyConfig,
) -> Vec<AppObservation> {
    let timeout_second = config.bootstrap_timeout.as_secs();
    let mut observations = Vec::new();
    for event in events {
        if event.percent == 100 {
            observations.push(AppObservation {
                second: event.second,
                state: AppBootstrapState::Ready,
            });
            return observations;
        }
        if event.second >= timeout_second {
            break;
        }
        observations.push(AppObservation {
            second: event.second,
            state: AppBootstrapState::Connecting,
        });
    }
    observations.push(AppObservation {
        second: timeout_second,
        state: AppBootstrapState::Failed,
    });
    observations
}

fn parse_bootstrap_stream(stream: &'static str) -> Vec<BootstrapEvent> {
    stream
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse_bootstrap_event)
        .collect()
}

fn parse_bootstrap_event(line: &'static str) -> BootstrapEvent {
    let (seconds, rest) = line
        .split_once("s ")
        .expect("fixture line has a second stamp");
    let whole_seconds = seconds
        .split_once('.')
        .map(|(whole, _)| whole)
        .unwrap_or(seconds)
        .parse::<u64>()
        .expect("fixture second stamp is numeric");
    let percent = rest
        .split_once("Bootstrapped ")
        .and_then(|(_, suffix)| suffix.split_once('%'))
        .map(|(percent, _)| percent)
        .expect("fixture line has an Arti bootstrap percent")
        .parse::<u8>()
        .expect("fixture bootstrap percent is numeric");
    BootstrapEvent {
        second: whole_seconds,
        percent,
    }
}

#[test]
fn fixture_has_the_required_cold_bootstrap_shape() {
    let events = parse_bootstrap_stream(TOR_4903_STREAM);
    let progress_events = events.iter().filter(|event| event.percent < 100).count();
    let ready_second = events
        .iter()
        .find(|event| event.percent == 100)
        .map(|event| event.second);

    assert_eq!(progress_events, 10);
    assert_eq!(ready_second, Some(75));
}
