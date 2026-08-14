use std::collections::HashSet;

const COVER_TEXT: &str = "task 3955 cover words repeat exactly for four rows";
const PRIVATE_WORDS: &str = "task 3955 exact private words opened";
const COPY_POSITIONS: [usize; 3] = [17, 88, 149];

#[derive(Clone, Debug)]
struct Row {
    row_index: usize,
    message_id: String,
    locator: String,
    carrier_text: String,
    carrier_key: String,
    attribution: Option<Attribution>,
}

#[derive(Clone, Debug)]
struct Attribution {
    row_index: usize,
    message_id: String,
    locator: String,
    carrier_key: String,
}

#[derive(Debug)]
struct ScreenRun {
    rows_on_screen: usize,
    matching_positions: Vec<usize>,
    opened_private_messages: Vec<&'static str>,
    refused_rows: usize,
    refusal_scope: &'static str,
}

fn carrier_key(text: &str) -> String {
    format!("carrier::{text}")
}

fn screen_rows(include_cover_copies: bool) -> Vec<Row> {
    let mut carriers = (0..200usize)
        .filter_map(|ordinary_index| {
            let is_copy = COPY_POSITIONS.contains(&ordinary_index);
            if is_copy && !include_cover_copies {
                None
            } else if is_copy {
                Some(COVER_TEXT.to_owned())
            } else {
                Some(format!("task 3955 ordinary visible row {ordinary_index:03}"))
            }
        })
        .collect::<Vec<_>>();
    carriers.push(COVER_TEXT.to_owned());

    carriers
        .into_iter()
        .enumerate()
        .map(|(row_index, carrier_text)| {
            let message_id = format!("{:018}", 395_500_000_000_000_000u64 + row_index as u64);
            let locator = format!("task-3955-locator-{row_index:03}");
            let carrier_key = carrier_key(&carrier_text);
            Row {
                row_index,
                message_id: message_id.clone(),
                locator: locator.clone(),
                carrier_text,
                carrier_key: carrier_key.clone(),
                attribution: Some(Attribution {
                    row_index,
                    message_id,
                    locator,
                    carrier_key,
                }),
            }
        })
        .collect()
}

fn batch_is_valid(rows: &[Row]) -> bool {
    if rows.is_empty() {
        return false;
    }
    let mut message_ids = HashSet::with_capacity(rows.len());
    let mut locators = HashSet::with_capacity(rows.len());
    let mut carriers = HashSet::with_capacity(rows.len());
    rows.iter().enumerate().all(|(row_index, row)| {
        let Some(attribution) = row.attribution.as_ref() else {
            return false;
        };
        attribution.row_index == row_index
            && attribution.row_index == row.row_index
            && attribution.message_id == row.message_id
            && attribution.locator == row.locator
            && attribution.carrier_key == row.carrier_key
            && message_ids.insert(attribution.message_id.clone())
            && locators.insert(attribution.locator.clone())
            && carriers.insert(attribution.carrier_key.clone())
    })
}

fn run_screen(rows: Vec<Row>) -> ScreenRun {
    let rows_on_screen = rows.len();
    let matching_positions = rows
        .iter()
        .enumerate()
        .filter_map(|(position, row)| (row.carrier_text == COVER_TEXT).then_some(position))
        .collect::<Vec<_>>();

    if !batch_is_valid(&rows) {
        return ScreenRun {
            rows_on_screen,
            matching_positions,
            opened_private_messages: Vec::new(),
            refused_rows: rows_on_screen,
            refusal_scope: "whole_screen",
        };
    }

    let opened_private_messages = rows
        .iter()
        .filter_map(|row| (row.carrier_text == COVER_TEXT).then_some(PRIVATE_WORDS))
        .collect::<Vec<_>>();
    ScreenRun {
        rows_on_screen,
        matching_positions,
        opened_private_messages,
        refused_rows: 0,
        refusal_scope: "none",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_3955_many_rows_that_look_alike_refuse_the_whole_screen() {
        let crowded = run_screen(screen_rows(true));
        assert_eq!(crowded.rows_on_screen, 201);
        assert_eq!(crowded.opened_private_messages.len(), 0);
        assert_eq!(crowded.refusal_scope, "whole_screen");
        assert_eq!(crowded.refused_rows, crowded.rows_on_screen);
        assert_eq!(crowded.matching_positions, vec![17, 88, 149, 200]);
        println!(
            "TASK3955 duplicate_run row_count={} opened_private_messages={} refusal_scope={} refused_rows={} matching_row_positions_zero_based={}",
            crowded.rows_on_screen,
            crowded.opened_private_messages.len(),
            crowded.refusal_scope,
            crowded.refused_rows,
            crowded
                .matching_positions
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );

        let control = run_screen(screen_rows(false));
        assert_eq!(control.rows_on_screen, 198);
        assert_eq!(control.matching_positions, vec![197]);
        assert_eq!(control.opened_private_messages, vec![PRIVATE_WORDS]);
        println!(
            "TASK3955 control_run copies_removed=3 row_count={} opened_private_messages={} matching_row_positions_zero_based={} exact_private_words=\"{}\"",
            control.rows_on_screen,
            control.opened_private_messages.len(),
            control
                .matching_positions
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(","),
            control.opened_private_messages[0]
        );
    }
}
