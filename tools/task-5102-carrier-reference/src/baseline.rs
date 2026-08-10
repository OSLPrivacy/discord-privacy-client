//! Immutable, reviewer-gated release baselines for bounded carrier captures.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const CANDIDATE_SCHEMA: &str = "osl-carrier-baseline-candidate-v1";
const RELEASE_SCHEMA: &str = "osl-carrier-release-baseline-v1";
const SIGNING_DOMAIN: &[u8] = b"OSL carrier release review v1\0";

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BaselineName {
    pub carrier: String,
    pub channel: String,
    pub version: String,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CandidateManifest {
    pub schema: String,
    pub name: BaselineName,
    pub parent_hash: Option<String>,
    pub capture_author: String,
    pub capture_manifest_sha256: String,
    pub carrier_png_sha256: String,
    pub candidate_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReviewApproval {
    pub reviewer: String,
    pub reviewer_key_id: String,
    pub signature_hex: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReleaseManifest {
    pub schema: String,
    pub name: BaselineName,
    pub parent_hash: Option<String>,
    pub baseline_hash: String,
    pub capture_author: String,
    pub reviewer: String,
    pub reviewer_key_id: String,
    pub signature_hex: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdvanceReceipt {
    pub old_hash: Option<String>,
    pub new_hash: String,
    pub capture_author: String,
    pub reviewer: String,
    pub signature_hex: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateActor {
    ReleaseReviewer,
    RuntimeAdaptation,
    CaptureAuthor,
    PassingDiff,
}

#[derive(Clone, Debug)]
pub struct ReviewTrustRoot {
    pub root_id: String,
    reviewers: BTreeMap<String, [u8; 32]>,
}

impl ReviewTrustRoot {
    pub fn configured(
        root_id: impl Into<String>,
        reviewers: impl IntoIterator<Item = (String, [u8; 32])>,
    ) -> Self {
        Self {
            root_id: root_id.into(),
            reviewers: reviewers.into_iter().collect(),
        }
    }

    fn reviewer_key(&self, reviewer: &str) -> Option<&[u8; 32]> {
        self.reviewers.get(reviewer)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum BaselineError {
    Invalid(&'static str),
    CandidateExists,
    CandidateMissing,
    CandidateTampered,
    UnauthorizedActor,
    MissingApproval,
    SelfReview,
    WrongTrustRoot,
    UnauthorizedReviewer,
    InvalidSignature,
    ParentChanged,
    AlreadyReleased,
    Io(String),
}

impl fmt::Display for BaselineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Invalid(reason) => return write!(f, "invalid carrier baseline: {reason}"),
            Self::CandidateExists => "candidate already exists",
            Self::CandidateMissing => "candidate is missing",
            Self::CandidateTampered => "candidate content hash disagrees",
            Self::UnauthorizedActor => "only an authorized release reviewer can advance a baseline",
            Self::MissingApproval => "authorized reviewer signature is required",
            Self::SelfReview => "capture author cannot review their own candidate",
            Self::WrongTrustRoot => "review signature names the wrong trust root",
            Self::UnauthorizedReviewer => "reviewer is not authorized by the configured trust root",
            Self::InvalidSignature => "release-review signature is invalid",
            Self::ParentChanged => "candidate parent is no longer current",
            Self::AlreadyReleased => "candidate was already released",
            Self::Io(reason) => return write!(f, "baseline storage failed: {reason}"),
        };
        f.write_str(text)
    }
}

impl std::error::Error for BaselineError {}

pub struct BaselineStore {
    root: PathBuf,
}

impl BaselineStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn create_candidate(
        &self,
        capture_manifest: &[u8],
        carrier_png: &[u8],
        capture_author: &str,
    ) -> Result<CandidateManifest, BaselineError> {
        if capture_author.trim().is_empty() {
            return Err(BaselineError::Invalid("capture author is required"));
        }
        let capture: serde_json::Value = serde_json::from_slice(capture_manifest)
            .map_err(|_| BaselineError::Invalid("capture manifest is not JSON"))?;
        let string = |field| {
            capture
                .get(field)
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or(BaselineError::Invalid("capture identity field is missing"))
        };
        let name = BaselineName {
            carrier: string("carrier")?,
            channel: string("channel")?,
            version: string("version")?,
            state: string("state")?,
        };
        let png_hash = hash(carrier_png);
        if capture.get("png_sha256").and_then(|value| value.as_str()) != Some(&png_hash) {
            return Err(BaselineError::Invalid("capture PNG hash disagrees"));
        }
        let parent_hash = self.current_hash(&name)?;
        let manifest_hash = hash(capture_manifest);
        let candidate_hash = compute_candidate_hash(
            &name,
            parent_hash.as_deref(),
            capture_author,
            &manifest_hash,
            &png_hash,
        )?;
        let candidate = CandidateManifest {
            schema: CANDIDATE_SCHEMA.to_owned(),
            name,
            parent_hash,
            capture_author: capture_author.to_owned(),
            capture_manifest_sha256: manifest_hash,
            carrier_png_sha256: png_hash,
            candidate_hash: candidate_hash.clone(),
        };
        let directory = self.root.join("candidates").join(&candidate_hash);
        fs::create_dir_all(self.root.join("candidates")).map_err(io_error)?;
        fs::create_dir(&directory).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                BaselineError::CandidateExists
            } else {
                io_error(error)
            }
        })?;
        let result = (|| {
            write_new(&directory.join("capture.manifest.json"), capture_manifest)?;
            write_new(&directory.join("carrier.png"), carrier_png)?;
            write_json_new(&directory.join("candidate.json"), &candidate)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&directory);
        }
        result.map(|_| candidate)
    }

    pub fn current_hash(&self, name: &BaselineName) -> Result<Option<String>, BaselineError> {
        let path = self.pointer_path(name)?;
        if !path.exists() {
            return Ok(None);
        }
        #[derive(Deserialize)]
        struct Pointer {
            baseline_hash: String,
        }
        let pointer: Pointer = serde_json::from_slice(&fs::read(path).map_err(io_error)?)
            .map_err(|_| BaselineError::Io("current pointer is malformed".to_owned()))?;
        Ok(Some(pointer.baseline_hash))
    }

    pub fn advance(
        &self,
        requested_hash: &str,
        actor: UpdateActor,
        approval: Option<&ReviewApproval>,
        trust_root: &ReviewTrustRoot,
    ) -> Result<AdvanceReceipt, BaselineError> {
        if actor != UpdateActor::ReleaseReviewer {
            return Err(BaselineError::UnauthorizedActor);
        }
        let approval = approval.ok_or(BaselineError::MissingApproval)?;
        let (candidate, capture_manifest, png) = self.load_candidate(requested_hash)?;
        if approval.reviewer == candidate.capture_author {
            return Err(BaselineError::SelfReview);
        }
        if approval.reviewer_key_id != trust_root.root_id {
            return Err(BaselineError::WrongTrustRoot);
        }
        let key_bytes = trust_root
            .reviewer_key(&approval.reviewer)
            .ok_or(BaselineError::UnauthorizedReviewer)?;
        let key =
            VerifyingKey::from_bytes(key_bytes).map_err(|_| BaselineError::InvalidSignature)?;
        let signature_bytes = decode_hex_64(&approval.signature_hex)?;
        let signature = Signature::from_bytes(&signature_bytes);
        key.verify(
            &review_payload(&candidate, &approval.reviewer, &approval.reviewer_key_id)?,
            &signature,
        )
        .map_err(|_| BaselineError::InvalidSignature)?;

        let current = self.current_hash(&candidate.name)?;
        if current.as_deref() == Some(requested_hash) {
            return Err(BaselineError::AlreadyReleased);
        }
        if current != candidate.parent_hash {
            return Err(BaselineError::ParentChanged);
        }

        let release = ReleaseManifest {
            schema: RELEASE_SCHEMA.to_owned(),
            name: candidate.name.clone(),
            parent_hash: candidate.parent_hash.clone(),
            baseline_hash: candidate.candidate_hash.clone(),
            capture_author: candidate.capture_author.clone(),
            reviewer: approval.reviewer.clone(),
            reviewer_key_id: approval.reviewer_key_id.clone(),
            signature_hex: approval.signature_hex.clone(),
        };
        let objects = self.root.join("baselines").join("objects");
        fs::create_dir_all(&objects).map_err(io_error)?;
        let object_dir = objects.join(requested_hash);
        fs::create_dir(&object_dir).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                BaselineError::AlreadyReleased
            } else {
                io_error(error)
            }
        })?;
        let installed = (|| {
            write_new(&object_dir.join("capture.manifest.json"), &capture_manifest)?;
            write_new(&object_dir.join("carrier.png"), &png)?;
            write_json_new(&object_dir.join("release.manifest.json"), &release)?;
            self.replace_pointer(&candidate.name, requested_hash)
        })();
        if installed.is_err() {
            let _ = fs::remove_dir_all(&object_dir);
            return installed.map(|_| unreachable!());
        }
        Ok(AdvanceReceipt {
            old_hash: candidate.parent_hash,
            new_hash: candidate.candidate_hash,
            capture_author: candidate.capture_author,
            reviewer: approval.reviewer.clone(),
            signature_hex: approval.signature_hex.clone(),
        })
    }

    fn load_candidate(
        &self,
        requested_hash: &str,
    ) -> Result<(CandidateManifest, Vec<u8>, Vec<u8>), BaselineError> {
        if !is_hash(requested_hash) {
            return Err(BaselineError::CandidateMissing);
        }
        let directory = self.root.join("candidates").join(requested_hash);
        let manifest_bytes = read_bounded(&directory.join("candidate.json"), 64 * 1024)?;
        let candidate: CandidateManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|_| BaselineError::CandidateTampered)?;
        let capture = read_bounded(&directory.join("capture.manifest.json"), 1024 * 1024)?;
        let png = read_bounded(&directory.join("carrier.png"), 64 * 1024 * 1024)?;
        let expected = compute_candidate_hash(
            &candidate.name,
            candidate.parent_hash.as_deref(),
            &candidate.capture_author,
            &hash(&capture),
            &hash(&png),
        )?;
        if candidate.schema != CANDIDATE_SCHEMA
            || candidate.candidate_hash != requested_hash
            || expected != requested_hash
            || candidate.capture_manifest_sha256 != hash(&capture)
            || candidate.carrier_png_sha256 != hash(&png)
        {
            return Err(BaselineError::CandidateTampered);
        }
        Ok((candidate, capture, png))
    }

    fn pointer_path(&self, name: &BaselineName) -> Result<PathBuf, BaselineError> {
        for part in [&name.carrier, &name.channel, &name.version, &name.state] {
            if part.is_empty()
                || part.len() > 128
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            {
                return Err(BaselineError::Invalid("unsafe baseline name"));
            }
        }
        // Version remains signed into each object, but the moving name omits
        // it so a carrier version update extends one parent-linked history.
        Ok(self.root.join("baselines").join("named").join(format!(
            "{}--{}--{}.json",
            name.carrier, name.channel, name.state
        )))
    }

    fn replace_pointer(
        &self,
        name: &BaselineName,
        baseline_hash: &str,
    ) -> Result<(), BaselineError> {
        #[derive(Serialize)]
        struct Pointer<'a> {
            baseline_hash: &'a str,
        }
        let path = self.pointer_path(name)?;
        let parent = path
            .parent()
            .ok_or(BaselineError::Io("pointer has no parent".to_owned()))?;
        fs::create_dir_all(parent).map_err(io_error)?;
        let temporary = parent.join(format!(".{}.tmp", baseline_hash));
        let bytes = serde_json::to_vec_pretty(&Pointer { baseline_hash })
            .map_err(|error| BaselineError::Io(error.to_string()))?;
        write_new(&temporary, &bytes)?;
        fs::rename(&temporary, &path).map_err(io_error)
    }
}

pub fn review_payload(
    candidate: &CandidateManifest,
    reviewer: &str,
    reviewer_key_id: &str,
) -> Result<Vec<u8>, BaselineError> {
    #[derive(Serialize)]
    struct Unsigned<'a> {
        schema: &'static str,
        name: &'a BaselineName,
        parent_hash: &'a Option<String>,
        baseline_hash: &'a str,
        capture_author: &'a str,
        reviewer: &'a str,
        reviewer_key_id: &'a str,
    }
    let mut bytes = SIGNING_DOMAIN.to_vec();
    bytes.extend(
        serde_json::to_vec(&Unsigned {
            schema: RELEASE_SCHEMA,
            name: &candidate.name,
            parent_hash: &candidate.parent_hash,
            baseline_hash: &candidate.candidate_hash,
            capture_author: &candidate.capture_author,
            reviewer,
            reviewer_key_id,
        })
        .map_err(|error| BaselineError::Io(error.to_string()))?,
    );
    Ok(bytes)
}

pub fn signature_hex(bytes: &[u8; 64]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn compute_candidate_hash(
    name: &BaselineName,
    parent_hash: Option<&str>,
    author: &str,
    manifest_hash: &str,
    png_hash: &str,
) -> Result<String, BaselineError> {
    #[derive(Serialize)]
    struct Address<'a> {
        schema: &'static str,
        name: &'a BaselineName,
        parent_hash: Option<&'a str>,
        capture_author: &'a str,
        capture_manifest_sha256: &'a str,
        carrier_png_sha256: &'a str,
    }
    let bytes = serde_json::to_vec(&Address {
        schema: CANDIDATE_SCHEMA,
        name,
        parent_hash,
        capture_author: author,
        capture_manifest_sha256: manifest_hash,
        carrier_png_sha256: png_hash,
    })
    .map_err(|error| BaselineError::Io(error.to_string()))?;
    Ok(hash(&bytes))
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn decode_hex_64(value: &str) -> Result<[u8; 64], BaselineError> {
    if value.len() != 128 {
        return Err(BaselineError::InvalidSignature);
    }
    let mut bytes = [0u8; 64];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| BaselineError::InvalidSignature)?;
    }
    Ok(bytes)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, BaselineError> {
    let file = fs::File::open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            BaselineError::CandidateMissing
        } else {
            io_error(error)
        }
    })?;
    if file.metadata().map_err(io_error)?.len() > limit {
        return Err(BaselineError::CandidateTampered);
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > limit {
        return Err(BaselineError::CandidateTampered);
    }
    Ok(bytes)
}

fn write_json_new(path: &Path, value: &impl Serialize) -> Result<(), BaselineError> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|error| BaselineError::Io(error.to_string()))?;
    write_new(path, &bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), BaselineError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

fn io_error(error: std::io::Error) -> BaselineError {
    BaselineError::Io(error.to_string())
}
