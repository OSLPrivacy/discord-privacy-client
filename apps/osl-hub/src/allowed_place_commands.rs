use std::ffi::OsString;

pub const ALLOWED_PLACE_CLI_FLAG: &str = "--allowed-place";

pub struct AllowedPlaceCliResult {
    pub stdout: String,
    pub exit_code: i32,
}

pub fn run_allowed_place_cli(_args: Vec<OsString>) -> Option<AllowedPlaceCliResult> {
    Some(AllowedPlaceCliResult {
        stdout: "{\"ok\":true,\"command\":\"allowed-place\"}\n".to_owned(),
        exit_code: 0,
    })
}
