//! Fail-closed validation for replies received from message services.
//!
//! A successful HTTP status is not enough authority to send, reveal, burn, or
//! offer a completed download. Callers describe the exact JSON object they
//! expect and receive a [`ValidatedServiceReply`] only after its media type,
//! framing, shape, and values have all been checked.

use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt;

pub const WRONG_SHAPED_REPLY: &str = "wrong-shaped";
pub const TRUNCATED_REPLY: &str = "truncated";
pub const WRONG_CONTENT_TYPE_REPLY: &str = "wrong-content-type";
pub const WRONG_VALUE_REPLY: &str = "wrong-value";

/// The four stable refusal categories used by every service operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceReplyRefusalKind {
    WrongShaped,
    Truncated,
    WrongContentType,
    WrongValue,
}

impl ServiceReplyRefusalKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::WrongShaped => WRONG_SHAPED_REPLY,
            Self::Truncated => TRUNCATED_REPLY,
            Self::WrongContentType => WRONG_CONTENT_TYPE_REPLY,
            Self::WrongValue => WRONG_VALUE_REPLY,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceReplyRefusal {
    kind: ServiceReplyRefusalKind,
    detail: String,
}

impl ServiceReplyRefusal {
    fn new(kind: ServiceReplyRefusalKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub const fn kind(&self) -> ServiceReplyRefusalKind {
        self.kind
    }

    /// Stable machine-readable refusal name. Details are deliberately kept
    /// separate so tests and callers never have to parse display text.
    pub const fn name(&self) -> &'static str {
        self.kind.name()
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ServiceReplyRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.name(), self.detail)
    }
}

impl std::error::Error for ServiceReplyRefusal {}

/// Raw response material at the service boundary.
///
/// `declared_byte_length` is the service's Content-Length value after the HTTP
/// layer has parsed it. The validator compares it with the bytes actually
/// delivered before attempting to parse JSON.
#[derive(Clone, Copy, Debug)]
pub struct ServiceReply<'a> {
    pub content_type: Option<&'a str>,
    pub declared_byte_length: Option<usize>,
    pub body: &'a [u8],
}

/// The exact object a particular service operation is willing to accept.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceReplyExpectation {
    content_type: String,
    fields: BTreeMap<String, Value>,
}

impl ServiceReplyExpectation {
    pub fn json<I, K>(content_type: impl Into<String>, fields: I) -> Self
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        Self {
            content_type: content_type.into(),
            fields: fields
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
        }
    }

    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    pub fn fields(&self) -> &BTreeMap<String, Value> {
        &self.fields
    }
}

/// Parsed fields that have passed the whole reply contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedServiceReply {
    byte_length: usize,
    fields: BTreeMap<String, Value>,
}

impl ValidatedServiceReply {
    pub const fn byte_length(&self) -> usize {
        self.byte_length
    }

    pub fn field(&self, name: &str) -> Option<&Value> {
        self.fields.get(name)
    }

    pub fn fields(&self) -> &BTreeMap<String, Value> {
        &self.fields
    }
}

/// Stateful boundary for direct message-service calls.
///
/// The counters deliberately live on the same object as the operations that
/// consume a reply. This makes it impossible for a caller to record a send as
/// successful, or to receive private read content, without first passing the
/// complete [`validate_service_reply`] contract.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DirectServiceReplyCalls {
    successful_send_count: usize,
    successful_read_count: usize,
}

impl DirectServiceReplyCalls {
    pub const fn new() -> Self {
        Self {
            successful_send_count: 0,
            successful_read_count: 0,
        }
    }

    pub const fn successful_send_count(&self) -> usize {
        self.successful_send_count
    }

    pub const fn successful_read_count(&self) -> usize {
        self.successful_read_count
    }

    /// Accept a direct send as successful only after its reply is valid.
    pub fn direct_send(
        &mut self,
        reply: ServiceReply<'_>,
        expected: &ServiceReplyExpectation,
    ) -> Result<ValidatedServiceReply, ServiceReplyRefusal> {
        let validated = validate_service_reply(reply, expected)?;
        self.successful_send_count += 1;
        Ok(validated)
    }

    /// Release private content from a direct read only after its reply is
    /// valid. On refusal, `private_content` is dropped inside this call and is
    /// never returned to the caller.
    pub fn direct_read<T>(
        &mut self,
        reply: ServiceReply<'_>,
        expected: &ServiceReplyExpectation,
        private_content: T,
    ) -> Result<T, ServiceReplyRefusal> {
        validate_service_reply(reply, expected)?;
        self.successful_read_count += 1;
        Ok(private_content)
    }
}

/// Check every part of a JSON service reply before returning any usable field.
pub fn validate_service_reply(
    reply: ServiceReply<'_>,
    expected: &ServiceReplyExpectation,
) -> Result<ValidatedServiceReply, ServiceReplyRefusal> {
    let Some(content_type) = reply.content_type else {
        return Err(ServiceReplyRefusal::new(
            ServiceReplyRefusalKind::WrongContentType,
            "Content-Type is missing",
        ));
    };
    if !same_media_type(content_type, expected.content_type()) {
        return Err(ServiceReplyRefusal::new(
            ServiceReplyRefusalKind::WrongContentType,
            format!(
                "expected {}, received {content_type}",
                expected.content_type()
            ),
        ));
    }

    let Some(declared_byte_length) = reply.declared_byte_length else {
        return Err(ServiceReplyRefusal::new(
            ServiceReplyRefusalKind::WrongShaped,
            "Content-Length is missing",
        ));
    };
    if declared_byte_length != reply.body.len() {
        return Err(ServiceReplyRefusal::new(
            ServiceReplyRefusalKind::Truncated,
            format!(
                "declared {declared_byte_length} bytes, received {}",
                reply.body.len()
            ),
        ));
    }

    let parsed: Value = serde_json::from_slice(reply.body).map_err(|error| {
        ServiceReplyRefusal::new(
            ServiceReplyRefusalKind::WrongShaped,
            format!("body is not one complete JSON value: {error}"),
        )
    })?;
    let Value::Object(fields) = parsed else {
        return Err(ServiceReplyRefusal::new(
            ServiceReplyRefusalKind::WrongShaped,
            "body is not a JSON object",
        ));
    };

    check_exact_field_names(&fields, expected.fields())?;
    for (name, expected_value) in expected.fields() {
        let actual_value = fields
            .get(name)
            .expect("the exact field-name check established this field");
        if actual_value != expected_value {
            return Err(ServiceReplyRefusal::new(
                ServiceReplyRefusalKind::WrongValue,
                format!("field {name:?} did not match its request"),
            ));
        }
    }

    Ok(ValidatedServiceReply {
        byte_length: declared_byte_length,
        fields: fields.into_iter().collect(),
    })
}

fn same_media_type(actual: &str, expected: &str) -> bool {
    let actual = actual.split(';').next().unwrap_or_default().trim();
    let expected = expected.split(';').next().unwrap_or_default().trim();
    !actual.is_empty() && actual.eq_ignore_ascii_case(expected)
}

fn check_exact_field_names(
    actual: &Map<String, Value>,
    expected: &BTreeMap<String, Value>,
) -> Result<(), ServiceReplyRefusal> {
    let mut actual_names = actual.keys().map(String::as_str).collect::<Vec<_>>();
    let expected_names = expected.keys().map(String::as_str).collect::<Vec<_>>();
    actual_names.sort_unstable();
    if actual_names != expected_names {
        return Err(ServiceReplyRefusal::new(
            ServiceReplyRefusalKind::WrongShaped,
            format!(
                "expected fields [{}], received [{}]",
                expected_names.join(", "),
                actual_names.join(", ")
            ),
        ));
    }
    Ok(())
}
