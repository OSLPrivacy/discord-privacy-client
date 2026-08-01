//! `LocalCoverModel` adapter backed by the selected llama.cpp runtime.
//!
//! The model pack is verified before this adapter can be constructed. Generation
//! is CPU-first, checks the existing cooperative cancellation/deadline control
//! between every decode, and selects tokens directly from `get_logits()` rather
//! than asking llama.cpp's sampler to truncate the distribution.

use std::num::NonZeroU32;
use std::path::Path;
use std::time::Instant;

use cover_draft::{GenerationControl, LocalCoverModel, ModelError, ModelInput, TrustedModelPack};
use encoding_rs::UTF_8;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::token::LlamaToken;
use zeroize::{Zeroize, Zeroizing};

use crate::logit_selection::select_from_full_logits;

const CONTEXT_TOKENS: u32 = 2_048;

/// The real optional local model implementation. It owns the CPU llama.cpp
/// backend and the loaded GGUF model, while a short-lived context is created per
/// request so no prior request's tokens can influence the next one.
pub struct LlamaCppLocalCoverModel {
    trusted_pack: TrustedModelPack,
    backend: LlamaBackend,
    model: LlamaModel,
}

impl LlamaCppLocalCoverModel {
    /// Loads a GGUF only after its metadata and digest have already produced a
    /// `TrustedModelPack`. The declared working-set ceiling is checked before
    /// allocating the llama.cpp model.
    pub fn load(
        trusted_pack: TrustedModelPack,
        artifact_path: impl AsRef<Path>,
    ) -> Result<Self, ModelError> {
        if trusted_pack.artifact_size() > trusted_pack.max_working_set_bytes() {
            return Err(ModelError::Failed);
        }

        let backend = LlamaBackend::init().map_err(|_| ModelError::Failed)?;
        let model =
            LlamaModel::load_from_file(&backend, artifact_path, &LlamaModelParams::default())
                .map_err(|_| ModelError::Failed)?;

        if model.size() > trusted_pack.max_working_set_bytes() {
            return Err(ModelError::Failed);
        }

        Ok(Self {
            trusted_pack,
            backend,
            model,
        })
    }

    fn prompt(input: ModelInput<'_>) -> Zeroizing<String> {
        let mut prompt = String::new();
        for entry in input.context() {
            if !prompt.is_empty() {
                prompt.push('\n');
            }
            prompt.push_str(entry);
        }
        Zeroizing::new(prompt)
    }
}

impl LocalCoverModel for LlamaCppLocalCoverModel {
    fn trusted_model_pack(&self) -> Option<&TrustedModelPack> {
        Some(&self.trusted_pack)
    }

    fn generate(
        &mut self,
        input: ModelInput<'_>,
        control: GenerationControl<'_>,
    ) -> Result<Zeroizing<String>, ModelError> {
        check_control(&control)?;

        let mut prompt = Self::prompt(input);
        let tokens = self
            .model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|_| ModelError::Failed)?;
        prompt.zeroize();
        if tokens.is_empty() {
            return Err(ModelError::Failed);
        }
        if tokens.len() >= CONTEXT_TOKENS as usize {
            return Err(ModelError::Failed);
        }

        let context_params = LlamaContextParams::default().with_n_ctx(Some(
            NonZeroU32::new(CONTEXT_TOKENS).expect("nonzero context size"),
        ));
        let mut context = self
            .model
            .new_context(&self.backend, context_params)
            .map_err(|_| ModelError::Failed)?;
        let next_position = tokens.len();
        let mut batch = LlamaBatch::new(tokens.len(), 1);
        let last = tokens.len() - 1;
        for (position, token) in tokens.into_iter().enumerate() {
            check_control(&control)?;
            batch
                .add(
                    token,
                    i32::try_from(position).map_err(|_| ModelError::Failed)?,
                    &[0],
                    position == last,
                )
                .map_err(|_| ModelError::Failed)?;
        }
        context.decode(&mut batch).map_err(|_| ModelError::Failed)?;

        let mut decoder = UTF_8.new_decoder();
        let mut output = Zeroizing::new(String::new());
        let output_token_limit = input
            .max_output_bytes
            .min(CONTEXT_TOKENS as usize - next_position);
        for position in 0..output_token_limit {
            if let Err(error) = check_control(&control) {
                output.zeroize();
                return Err(error);
            }

            let selection =
                select_from_full_logits(context.get_logits()).ok_or(ModelError::Failed)?;
            let token = LlamaToken::new(
                i32::try_from(selection.token_index).map_err(|_| ModelError::Failed)?,
            );
            if self.model.is_eog_token(token) {
                break;
            }
            let piece = self
                .model
                .token_to_piece(token, &mut decoder, true, None)
                .map_err(|_| ModelError::Failed)?;
            if output.len().saturating_add(piece.len()) > input.max_output_bytes {
                break;
            }
            output.push_str(&piece);

            batch.clear();
            batch
                .add(
                    token,
                    i32::try_from(next_position + position).map_err(|_| ModelError::Failed)?,
                    &[0],
                    true,
                )
                .map_err(|_| ModelError::Failed)?;
            context.decode(&mut batch).map_err(|_| ModelError::Failed)?;
        }

        if let Err(error) = check_control(&control) {
            output.zeroize();
            return Err(error);
        }
        Ok(output)
    }
}

fn check_control(control: &GenerationControl<'_>) -> Result<(), ModelError> {
    if control.cancellation.is_cancelled() {
        Err(ModelError::Cancelled)
    } else if Instant::now() >= control.deadline {
        Err(ModelError::DeadlineExceeded)
    } else {
        Ok(())
    }
}
