use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildProofInput {
    pub build_fingerprint: String,
    pub device_id: String,
    pub person_id: String,
    pub made_at_unix_seconds: u64,
    pub stops_counting_at_unix_seconds: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProof {
    pub build_fingerprint: String,
    pub device_id: String,
    pub person_id: String,
    pub made_at_unix_seconds: u64,
    pub stops_counting_at_unix_seconds: u64,
}

pub fn make_build_proof(input: BuildProofInput) -> Result<BuildProof, String> {
    validate_fingerprint(&input.build_fingerprint)?;
    validate_identifier("device ID", &input.device_id)?;
    validate_identifier("person ID", &input.person_id)?;

    if input.stops_counting_at_unix_seconds <= input.made_at_unix_seconds {
        return Err("stops-counting-at time must be later than made-at time".to_owned());
    }

    Ok(BuildProof {
        build_fingerprint: input.build_fingerprint,
        device_id: input.device_id,
        person_id: input.person_id,
        made_at_unix_seconds: input.made_at_unix_seconds,
        stops_counting_at_unix_seconds: input.stops_counting_at_unix_seconds,
    })
}

fn validate_fingerprint(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(
            "build fingerprint must be exactly 64 lowercase hexadecimal characters".to_owned(),
        );
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    if value != value.trim() {
        return Err(format!("{label} must not start or end with whitespace"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{label} must not contain control characters"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_input() -> BuildProofInput {
        BuildProofInput {
            build_fingerprint: "a".repeat(64),
            device_id: "device-3096".to_owned(),
            person_id: "person-3096".to_owned(),
            made_at_unix_seconds: 1_754_828_400,
            stops_counting_at_unix_seconds: 1_754_914_800,
        }
    }

    #[test]
    fn stop_time_must_be_later_than_made_time() {
        let input = BuildProofInput {
            stops_counting_at_unix_seconds: 1_754_828_400,
            ..valid_input()
        };
        assert_eq!(
            make_build_proof(input).unwrap_err(),
            "stops-counting-at time must be later than made-at time"
        );
    }
}
