//! **D-215.** A cover-text judge whose numbers can be believed — and, first, the
//! control that decides whether they can.
//!
//! # Why this file exists
//!
//! The judge this project has quoted every cover-text number from is an add-0.5
//! smoothed word-bigram model trained on 400 lines of chat
//! (`corpus/chat-en-expanded-v1.txt`). B0-06 showed it calls **human writing
//! "generated" 200 times out of 200**: `stego/src/bigram_corpus.txt`, hand-written
//! chat of a different provenance, scores 1.000 against held-out lines of the
//! reader's own corpus, under the default scorer *and* under a vocabulary-blind
//! variant added to rule out the obvious explanation.
//!
//! It detects **membership of its own corpus**, not machine authorship. And the
//! control that should have caught this — `the_reader_cannot_tell_real_from_real`
//! — compares the corpus **against itself**, so it is a *bias* gate that was read
//! as a *validity* gate.
//!
//! # What is wrong with it, mechanically
//!
//! Its alphabet is **open**. A word the reader never trained on maps to `<unk>`,
//! and every transition touching it is scored through `α / (c(x) + α·V)` — a floor
//! that has nothing to do with whether the sentence is English. With 400 lines of
//! training text the vocabulary is a few thousand types, so *any* text from
//! outside that corpus is mostly floor. The score it computes is, to a first
//! approximation, **vocabulary overlap with `chat-en-expanded-v1.txt`**.
//!
//! The vocabulary-blind variant (`mean_log_prob_known_only`) does not rescue it,
//! and B0-06 proved that too: dropping unknown-word transitions still leaves a
//! 400-line bigram table that has never read the transitions of fluent English,
//! so it penalises attested human transitions it happens not to have seen.
//!
//! # THE ANSWER, BEFORE ANY OF THE MACHINERY
//!
//! **No scorer in this file may have a number quoted from it, including the two
//! new ones.** `QUOTABLE` is empty and Control 2 asserts that it is empty because
//! the measurement says so. Three human corpora, each in turn excluded from
//! training and used as the control, 200 length-matched forced-choice trials per
//! row, both sides always human — and the worst distance from the only correct
//! answer of 0.500 is:
//!
//! ```text
//!   legacy default          0.500     legacy known-only       0.435
//!   frame-bigram density    0.470     frame-trigram density   0.460
//!   frame-bigram PMI        0.325     frame-trigram PMI       0.460
//! ```
//!
//! against a tolerance of 0.20 that admits anything in `[0.30, 0.70]`. Every one
//! of them is a provenance detector. The new judge is a large and demonstrable
//! improvement on three separate axes and it is **still not good enough to quote**,
//! and saying so is worth more than a number would be.
//!
//! What follows is what was built, what it fixed, and exactly where it stops.
//!
//! # Attempt 1: close the alphabet — necessary, not sufficient
//!
//! Map every word to one of ~200 symbols: English **closed-class** words
//! (determiner, pronoun, preposition, auxiliary, modal, conjunction, negator,
//! wh-word, common contractions) map to themselves, everything else maps to a
//! single `<OPEN>`. Closed-class inventories are a property of the **language**,
//! not of a corpus, so a word the model has never met is not a hole in its
//! vocabulary — it is `<OPEN>`, which it has read tens of thousands of times.
//!
//! What survives is **syntactic frame**: `a <OPEN>`, `<OPEN> of the`, `i do not`.
//!
//! Trained on the same 400 lines as the broken judge, `density_score` **failed
//! Control 2 at 1.000** on both outside provenances, as hard as the scorer it
//! replaced. Closing the alphabet removes vocabulary *identity* as a channel and
//! leaves vocabulary *density*: `<OPEN> <OPEN>` is the commonest transition there
//! is, so the score tracks how many function words a text contains — 45.9% in the
//! judge's own corpus, 54.0% in the outside hand-written corpus, 61.1% in scraped
//! SMS. It also rated uniform random words from its own vocabulary as **more human
//! than real text** (Control 3, rate 0.045), because random chat words are 91%
//! open-class.
//!
//! # Attempt 2: score the order, not the composition
//!
//! `pmi` is a likelihood **ratio against the model's own marginal**, not a
//! likelihood: `mean[ ln P(next | context) − ln P(next) ]`. Order carries
//! information; composition does not. Word salad is near zero by construction
//! rather than by threshold.
//!
//! This fixed Control 1 and Control 3 outright — it is the only scorer here that
//! flags both shuffled human text (1.000) and vocabulary-matched random words
//! (1.000) while staying unbiased inside its own corpus (0.460). It did **not**
//! fix Control 2.
//!
//! # Attempt 3: pool provenances and give it more data
//!
//! `the_control_is_passable_only_above_a_training_scale` measures the mechanism
//! directly. The channel is coverage — how much of an outside corpus's syntax the
//! judge has ever read:
//!
//! ```text
//!   400 lines, one provenance   70.0% of the control's frames seen   rate 0.635
//!  1845 lines, two provenances  98.1%                                rate 0.170
//! ```
//!
//! Coverage closes and the rate keeps moving — straight through 0.500 and out the
//! other side. More data does not converge this judge on fairness; it swaps which
//! corpus it prefers. That is the finding that ends the line of attack.
//!
//! # Why the control is leave-one-out and two-sided
//!
//! An earlier form of Control 2 gated one corpus in one direction at a ceiling of
//! 0.70, and **that gate is passable by accident**: swapping the sides of a forced
//! choice turns rate `r` into `1 − r`. The frame-PMI trigram scorer sat at
//! **0.000** against `bigram_corpus.txt` and **0.980** on the same two corpora with
//! the sides swapped. One number, dressed as a pass. `|rate − 0.500|` regardless of
//! sign is the only honest statistic, and a row at 0.000 is exactly as broken as a
//! row at 1.000.
//!
//! ## Parameters, fixed before measuring
//!
//! `BIGRAM_LAMBDA = 0.7`; trigram weights `0.5 / 0.3 / 0.2`; `PROVENANCE_TOLERANCE
//! = 0.20`; `SALAD_FLOOR = 0.85`. None was adjusted after seeing a result, and the
//! measured numbers are nowhere near any of them, so no conclusion here turns on a
//! threshold. There is also **no threshold on the score itself**: every number is a
//! forced choice between two texts, so the judge only ever orders a pair.
//!
//! # The controls, and they all run by default
//!
//! | control | requirement | asserted on |
//! |---|---|---|
//! | 1 — bias inside the judge's own corpora | ≈0.500 | frame PMI |
//! | 2 — human text of a provenance excluded from training, **all three ways round** | within 0.20 of 0.500 | agreement with `QUOTABLE` |
//! | 3 — word salad: human words, shuffled | **must** be flagged ≥0.85 | frame PMI |
//! | 3 — word salad: uniform draws from the judge's own vocabulary | **must** be flagged ≥0.85 | frame PMI |
//!
//! Control 3 is not decoration. Control 2 alone is passed perfectly by a judge that
//! says "human" to everything, and Control 3 is what stops this file being made
//! green by tuning: a scorer bent toward provenance-blindness loses the ability to
//! flag salad, and both are asserted at once.
//!
//! # The old scorer is kept, not deleted
//!
//! `LegacyReader` is the D-161/B0-06 reader **verbatim**, including
//! `mean_log_prob_known_only`. It is run through every control beside the new ones,
//! and `the_old_protocol_still_reproduces_its_published_numbers` **asserts** that it
//! still returns D-161's exact published row (`1.000`, `+0.8306`). D-215 can
//! therefore be re-observed on demand instead of taken on trust, and the
//! re-measured rows are known to be comparable with the withdrawn ones.
//! NEVER REMOVE A SPEC.
//!
//! # Running the rows
//!
//! ```text
//! cargo test -p cover-ai --test cover_judge -- --nocapture --test-threads=1
//!
//! # the B0-06 model-written row, from the covers that lane dumped:
//! OSL_COVER_DUMP=plan-repo/plan-test/runlogs/b006-model-covers.txt \
//!   cargo test -p cover-ai --test cover_judge -- --nocapture --test-threads=1
//! ```

use stego::bigram;

/// Payload width of one shipping carrier chunk, from `bigram::TOKEN_PAYLOAD_BITS`.
const PAYLOAD_BITS: u32 = bigram::TOKEN_PAYLOAD_BITS;

/// Lines of the real-chat corpus used to train the judge. The rest are its
/// held-out "real" items. Same split as the D-161/B0-06 harness, deliberately.
const READER_TRAINING_LINES: usize = 400;

/// The judge's own corpus. Training and held-out items both come from here.
const REAL_CHAT: &str = include_str!("../corpus/chat-en-expanded-v1.txt");

/// **Control corpus 1 — different provenance.** Hand-written texting-register
/// English (`lol`, `ngl`, `dont`), written for the word-table codec, never seen
/// by this judge. Sharing vocabulary with the codec makes this control *harder*
/// in the right direction: a judge that passes it while still flagging word-table
/// covers is demonstrably using word order and not word identity.
const OTHER_HUMAN_CHAT: &str = include_str!("../../stego/src/bigram_corpus.txt");

/// **Control corpus 2 — external provenance.** Real scraped human chat: NPS Chat
/// plus the UCI SMS ham set, filtered but never rewritten, as assembled by the
/// D-116 lane (`fix/d116-cover-corpus`, `crates/stego/src/bigram_corpus.txt`).
/// Nobody on this project wrote a line of it, which is the point: control corpus
/// 1 could in principle share an author with the judge's corpus, and this one
/// cannot. It is also noticeably *less* well-formed than either — typos, missing
/// copulas, `your`/`you're` — so it is the control most likely to fail for an
/// honest reason.
const EXTERNAL_HUMAN_CHAT: &str = include_str!("fixtures/external-human-chat-v1.txt");

/// Forced-choice trials per reported rate. Report the count beside the rate.
fn trials() -> usize {
    std::env::var("OSL_DISCRIMINATION_TRIALS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|count| *count > 0)
        .unwrap_or(200)
}

/// Lowercase words with punctuation removed, apostrophes kept.
///
/// Byte-identical to the D-161 harness's `normalise`, so both judges see exactly
/// the same token stream and any difference between them is the model and not the
/// tokenisation.
fn normalise(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|character| character.is_alphanumeric() || *character == '\'')
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

// ==========================================================================
// The old scorer, kept verbatim so its failure is visible beside the new one
// ==========================================================================

/// The D-161 / B0-06 reader, unchanged. **This is the instrument D-215 withdrew.**
///
/// It is retained and exercised — not deleted — for three reasons: every earlier
/// number was produced by it and must stay reproducible; the new judge's claim is
/// comparative and needs the thing it is compared against; and a defect that can
/// be re-observed on demand does not have to be believed on the strength of a
/// prose entry.
struct LegacyReader {
    counts: std::collections::HashMap<(usize, usize), u32>,
    context_totals: std::collections::HashMap<usize, u32>,
    vocabulary: std::collections::HashMap<String, usize>,
}

const UNKNOWN: usize = 0;
const ALPHA: f64 = 0.5;

impl LegacyReader {
    fn train(lines: &[&str]) -> Self {
        let mut vocabulary = std::collections::HashMap::new();
        vocabulary.insert("<unk>".to_owned(), UNKNOWN);
        let mut counts = std::collections::HashMap::new();
        let mut context_totals = std::collections::HashMap::new();
        for line in lines {
            let words = normalise(line);
            for pair in words.windows(2) {
                let next = vocabulary.len();
                let left = *vocabulary.entry(pair[0].clone()).or_insert(next);
                let next = vocabulary.len();
                let right = *vocabulary.entry(pair[1].clone()).or_insert(next);
                *counts.entry((left, right)).or_insert(0) += 1;
                *context_totals.entry(left).or_insert(0) += 1;
            }
        }
        Self {
            counts,
            context_totals,
            vocabulary,
        }
    }

    fn index(&self, word: &str) -> usize {
        self.vocabulary.get(word).copied().unwrap_or(UNKNOWN)
    }

    fn unknown_word_rate(&self, text: &str) -> f64 {
        let words = normalise(text);
        if words.is_empty() {
            return 0.0;
        }
        let unknown = words
            .iter()
            .filter(|word| self.index(word) == UNKNOWN)
            .count();
        unknown as f64 / words.len() as f64
    }

    /// The default scorer. Open alphabet, add-0.5 floor over the whole vocabulary.
    fn mean_log_prob(&self, text: &str) -> f64 {
        let words = normalise(text);
        if words.len() < 2 {
            return f64::NEG_INFINITY;
        }
        let vocabulary = self.vocabulary.len() as f64;
        let mut total = 0.0;
        for pair in words.windows(2) {
            let left = self.index(&pair[0]);
            let right = self.index(&pair[1]);
            let joint = f64::from(self.counts.get(&(left, right)).copied().unwrap_or(0));
            let context = f64::from(self.context_totals.get(&left).copied().unwrap_or(0));
            total += ((joint + ALPHA) / (context + ALPHA * vocabulary)).ln();
        }
        total / (words.len() - 1) as f64
    }

    /// The vocabulary-blind variant B0-06 added to rule out the obvious
    /// explanation. It scores only transitions between words the reader knows.
    /// It calibrates better than the default and **fails the provenance control
    /// exactly as hard**, which is what proved the defect is the corpus and not
    /// the floor.
    fn mean_log_prob_known_only(&self, text: &str) -> Option<f64> {
        let words = normalise(text);
        let vocabulary = self.vocabulary.len() as f64;
        let mut total = 0.0;
        let mut counted = 0usize;
        for pair in words.windows(2) {
            let left = self.index(&pair[0]);
            let right = self.index(&pair[1]);
            if left == UNKNOWN || right == UNKNOWN {
                continue;
            }
            let joint = f64::from(self.counts.get(&(left, right)).copied().unwrap_or(0));
            let context = f64::from(self.context_totals.get(&left).copied().unwrap_or(0));
            total += ((joint + ALPHA) / (context + ALPHA * vocabulary)).ln();
            counted += 1;
        }
        (counted >= 5).then(|| total / counted as f64)
    }
}

// ==========================================================================
// The new judge: a closed-alphabet syntactic-frame model
// ==========================================================================

/// English closed-class words. A property of the language, not of any corpus —
/// which is the entire reason this judge can be provenance-invariant.
///
/// Forms without apostrophes (`dont`, `im`, `youre`) are included because chat
/// omits them and `normalise` does not restore them. Three otherwise-obvious
/// entries are **deliberately absent** — `well`, `ill`, `id` — because their
/// apostrophe-less contraction collides with a common open-class word, and
/// mapping "I feel well" onto the `we'll` symbol would put corpus-specific
/// content back into the alphabet.
const CLOSED_CLASS: &[&str] = &[
    // determiners and quantifiers
    "a", "an", "the", "this", "that", "these", "those", "my", "your", "his", "her", "its", "our",
    "their", "some", "any", "every", "no", "each", "another", "all", "both", "much", "many",
    "more", "most", "few", "fewer", "less", "least", "such", "own", "same", "other", "either",
    "neither", "enough",
    // pronouns
    "i", "you", "he", "she", "it", "we", "they", "me", "him", "us", "them", "who", "whom", "whose",
    "someone", "something", "anyone", "anything", "everyone", "everything", "nothing", "nobody",
    "myself", "yourself", "himself", "herself", "itself", "ourselves", "themselves", "there",
    "here", "mine", "yours", "hers", "ours", "theirs", "one",
    // prepositions and particles
    "of", "in", "on", "at", "to", "for", "with", "from", "by", "about", "into", "onto", "over",
    "under", "after", "before", "between", "through", "during", "without", "against", "across",
    "around", "behind", "beside", "near", "since", "until", "till", "upon", "within", "among",
    "along", "toward", "towards", "off", "out", "up", "down", "back", "away", "again", "together",
    "per", "via", "like",
    // auxiliaries and modals
    "am", "is", "are", "was", "were", "be", "been", "being", "do", "does", "did", "done", "have",
    "has", "had", "having", "will", "would", "can", "could", "should", "shall", "may", "might",
    "must", "ought", "gonna", "gotta", "wanna",
    // conjunctions and complementisers
    "and", "or", "but", "so", "because", "if", "when", "while", "although", "though", "unless",
    "whether", "than", "as", "nor", "yet", "once", "whenever", "wherever", "however",
    // negation, wh-words, degree
    "not", "never", "how", "why", "where", "what", "which", "too", "very", "just", "only", "also",
    "still", "already", "even", "quite", "rather", "almost", "always", "sometimes", "often",
    // contractions, apostrophised and not
    "i'm", "im", "it's", "that's", "thats", "you're", "youre", "we're", "they're", "theyre",
    "i've", "ive", "you've", "youve", "we've", "weve", "they've", "theyve", "can't", "cant",
    "don't", "dont", "doesn't", "doesnt", "didn't", "didnt", "won't", "wont", "isn't", "isnt",
    "aren't", "arent", "wasn't", "wasnt", "weren't", "werent", "haven't", "havent", "hasn't",
    "hasnt", "hadn't", "hadnt", "shouldn't", "shouldnt", "wouldn't", "wouldnt", "couldn't",
    "couldnt", "i'll", "you'll", "youll", "he'll", "she'll", "we'll", "they'll", "theyll", "i'd",
    "you'd", "youd", "he'd", "she'd", "we'd", "they'd", "theyd", "he's", "hes", "she's", "shes",
    "there's", "theres", "what's", "whats", "let's", "lets", "who's", "whos", "ain't", "aint",
];

/// Every word outside `CLOSED_CLASS` maps here. This symbol is why a word the
/// judge has never met is not a hole in its vocabulary.
const OPEN: usize = 0;
/// Sentence boundaries, so "starts with a preposition" and "ends with a
/// determiner" are frames the model can hold an opinion about.
const BOS: usize = 1;
const EOS: usize = 2;
const FIRST_CLOSED: usize = 3;

/// Fixed before any measurement. See the module docs.
const BIGRAM_LAMBDA: f64 = 0.7;
const TRIGRAM_WEIGHTS: [f64; 3] = [0.5, 0.3, 0.2];

/// Fewest scoreable transitions before a text gets a score at all.
const MIN_TRANSITIONS: usize = 5;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    Bigram,
    Trigram,
}

impl Order {
    fn label(self) -> &'static str {
        match self {
            Order::Bigram => "frame-bigram",
            Order::Trigram => "frame-trigram",
        }
    }
}

/// Interpolated n-gram model over the closed symbol alphabet.
struct FrameJudge {
    alphabet: std::collections::HashMap<&'static str, usize>,
    alphabet_size: usize,
    unigram: Vec<u32>,
    unigram_total: u32,
    bigram: std::collections::HashMap<(usize, usize), u32>,
    bigram_context: Vec<u32>,
    trigram: std::collections::HashMap<(usize, usize, usize), u32>,
    trigram_context: std::collections::HashMap<(usize, usize), u32>,
}

impl FrameJudge {
    fn train(lines: &[&str]) -> Self {
        // `CLOSED_CLASS` may repeat a word across categories (`no`, `that`,
        // `there`); dedup, then assign dense ids so `unigram` can be a flat
        // vector and the symbol count stays honest.
        let ordered: std::collections::BTreeSet<&'static str> =
            CLOSED_CLASS.iter().copied().collect();
        let mut alphabet = std::collections::HashMap::new();
        let mut next = FIRST_CLOSED;
        for word in ordered {
            alphabet.insert(word, next);
            next += 1;
        }
        let alphabet_size = next;

        let mut judge = Self {
            alphabet,
            alphabet_size,
            unigram: vec![0; alphabet_size],
            unigram_total: 0,
            bigram: std::collections::HashMap::new(),
            bigram_context: vec![0; alphabet_size],
            trigram: std::collections::HashMap::new(),
            trigram_context: std::collections::HashMap::new(),
        };
        for line in lines {
            let symbols = judge.symbols(line);
            for &symbol in &symbols {
                judge.unigram[symbol] += 1;
                judge.unigram_total += 1;
            }
            for pair in symbols.windows(2) {
                *judge.bigram.entry((pair[0], pair[1])).or_insert(0) += 1;
                judge.bigram_context[pair[0]] += 1;
            }
            for triple in symbols.windows(3) {
                *judge
                    .trigram
                    .entry((triple[0], triple[1], triple[2]))
                    .or_insert(0) += 1;
                *judge
                    .trigram_context
                    .entry((triple[0], triple[1]))
                    .or_insert(0) += 1;
            }
        }
        judge
    }

    /// `BOS`, the mapped words, `EOS`.
    fn symbols(&self, text: &str) -> Vec<usize> {
        let mut out = vec![BOS];
        out.extend(
            normalise(text)
                .iter()
                .map(|word| self.alphabet.get(word.as_str()).copied().unwrap_or(OPEN)),
        );
        out.push(EOS);
        out
    }

    fn unigram_prob(&self, symbol: usize) -> f64 {
        (f64::from(self.unigram[symbol]) + 1.0)
            / (f64::from(self.unigram_total) + self.alphabet_size as f64)
    }

    fn bigram_prob(&self, left: usize, right: usize) -> f64 {
        let context = f64::from(self.bigram_context[left]);
        let backoff = self.unigram_prob(right);
        if context == 0.0 {
            return backoff;
        }
        let joint = f64::from(self.bigram.get(&(left, right)).copied().unwrap_or(0));
        BIGRAM_LAMBDA * (joint / context) + (1.0 - BIGRAM_LAMBDA) * backoff
    }

    fn trigram_prob(&self, first: usize, second: usize, third: usize) -> f64 {
        let bigram = self.bigram_prob(second, third);
        let unigram = self.unigram_prob(third);
        let context = f64::from(
            self.trigram_context
                .get(&(first, second))
                .copied()
                .unwrap_or(0),
        );
        let conditional = if context == 0.0 {
            0.0
        } else {
            f64::from(
                self.trigram
                    .get(&(first, second, third))
                    .copied()
                    .unwrap_or(0),
            ) / context
        };
        TRIGRAM_WEIGHTS[0] * conditional + TRIGRAM_WEIGHTS[1] * bigram + TRIGRAM_WEIGHTS[2] * unigram
    }

    /// **REJECTED SCORER — kept as evidence, never asserted on.**
    ///
    /// Mean log-probability per transition over the frame alphabet. This was the
    /// first attempt at the D-215 fix and **it failed Control 2 at 1.000**, on
    /// both outside provenances, exactly as hard as the legacy scorer.
    ///
    /// The reason is worth keeping in the file rather than in a commit message.
    /// Closing the alphabet removes vocabulary *identity* as a channel but not
    /// vocabulary *density*: `<OPEN> <OPEN>` is by far the commonest transition,
    /// so this score is dominated by how many function words a text contains.
    /// Measured on the same run: closed-class rate is **46.5%** in the judge's own
    /// corpus, **54.0%** in the hand-written outside corpus and **61.1%** in
    /// scraped SMS. That is a register difference between two human corpora, and
    /// this scorer reads it as generation. It also scores uniform random words
    /// drawn from its own vocabulary as **more** human than real text (rate 0.045)
    /// because such text is 91% `<OPEN>`.
    ///
    /// Closing the alphabet was necessary and not sufficient. `pmi` below is the
    /// sufficient part.
    fn density_score(&self, text: &str, order: Order) -> Option<f64> {
        let symbols = self.symbols(text);
        let mut total = 0.0;
        let mut counted = 0usize;
        match order {
            Order::Bigram => {
                for pair in symbols.windows(2) {
                    total += self.bigram_prob(pair[0], pair[1]).ln();
                    counted += 1;
                }
            }
            Order::Trigram => {
                for triple in symbols.windows(3) {
                    total += self.trigram_prob(triple[0], triple[1], triple[2]).ln();
                    counted += 1;
                }
            }
        }
        (counted >= MIN_TRANSITIONS).then(|| total / counted as f64)
    }

    /// **THE SCORER.** Mean pointwise mutual information per transition:
    ///
    /// ```text
    ///   (1/N) · Σ [ ln P(next | context) − ln P(next) ]
    /// ```
    ///
    /// Not a likelihood — a **likelihood ratio against the model's own marginal**.
    /// It asks one question: *does knowing the previous frame symbol help predict
    /// the next one?* Order carries information; composition does not.
    ///
    /// This is what makes the judge provenance-invariant rather than merely
    /// vocabulary-invariant. Every term is a difference between a conditional and
    /// its own marginal, so a text made of the same symbols in a random order
    /// scores ≈ 0 **whatever those symbols are**. A corpus with 61% function words
    /// and a corpus with 46% function words are therefore on the same scale, which
    /// `density_score` above is not. Word salad is near zero by construction, not
    /// by threshold.
    ///
    /// It is a strictly weaker instrument than a likelihood, and deliberately so:
    /// everything it can see is a property of **word order within a short window**.
    /// Nothing about topic, register, or where a sentence stops survives into it.
    /// See `what_this_judge_cannot_see`.
    fn pmi(&self, text: &str, order: Order) -> Option<f64> {
        let symbols = self.symbols(text);
        let mut total = 0.0;
        let mut counted = 0usize;
        match order {
            Order::Bigram => {
                for pair in symbols.windows(2) {
                    total += self.bigram_prob(pair[0], pair[1]).ln() - self.unigram_prob(pair[1]).ln();
                    counted += 1;
                }
            }
            Order::Trigram => {
                for triple in symbols.windows(3) {
                    total += self.trigram_prob(triple[0], triple[1], triple[2]).ln()
                        - self.unigram_prob(triple[2]).ln();
                    counted += 1;
                }
            }
        }
        (counted >= MIN_TRANSITIONS).then(|| total / counted as f64)
    }

    /// Diagnostic: share of a text's frame bigrams the judge has actually seen.
    /// Reported beside every rate, the way the old harness reported unknown-word
    /// rate — it is what separates "the frames are wrong" from "the judge has
    /// never met these frames".
    fn attested_frame_rate(&self, text: &str) -> f64 {
        let symbols = self.symbols(text);
        if symbols.len() < 2 {
            return 0.0;
        }
        let attested = symbols
            .windows(2)
            .filter(|pair| self.bigram.contains_key(&(pair[0], pair[1])))
            .count();
        attested as f64 / (symbols.len() - 1) as f64
    }

    /// Share of a text's words that are closed-class. A diagnostic only: it is
    /// the one channel through which vocabulary could still reach the score, so
    /// it is reported rather than left implicit.
    fn closed_class_rate(&self, text: &str) -> f64 {
        let words = normalise(text);
        if words.is_empty() {
            return 0.0;
        }
        let closed = words
            .iter()
            .filter(|word| self.alphabet.contains_key(word.as_str()))
            .count();
        closed as f64 / words.len() as f64
    }
}

// ==========================================================================
// Trial machinery
// ==========================================================================

/// Deterministic bit source; the D-161 seed, so the word-table row below is
/// directly comparable with the one in `tasklogs/B0-06.md`.
struct Payloads(u64);

impl Payloads {
    fn next_bit(&mut self) -> bool {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 & 1 == 1
    }

    fn next_cover(&mut self) -> String {
        let bits: Vec<bool> = (0..PAYLOAD_BITS).map(|_| self.next_bit()).collect();
        bigram::render_words(&bigram::arithmetic_decode_bits(&bits, PAYLOAD_BITS))
    }
}

fn lines_of(corpus: &'static str) -> Vec<&'static str> {
    corpus
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

/// One item drawn from `source`, length-matched to `match_words` when asked.
/// Identical to the D-161 harness's `real_item`, so the length-matching protocol
/// is the same protocol and not a similar one.
fn item(source: &[&str], cursor: &mut usize, match_words: Option<usize>) -> String {
    let mut out = String::new();
    let mut count = 0;
    loop {
        let line = source[*cursor % source.len()];
        *cursor += 1;
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(line);
        count += normalise(line).len();
        match match_words {
            None => break,
            Some(target) if count >= target => break,
            Some(_) => {}
        }
    }
    out
}

/// What a scorer must offer to be run through the controls: order a pair.
trait Judge {
    fn name(&self) -> String;
    /// Higher is "more human". `None` = not scoreable.
    fn score(&self, text: &str) -> Option<f64>;
}

struct LegacyDefault<'a>(&'a LegacyReader);
struct LegacyKnownOnly<'a>(&'a LegacyReader);
struct FrameDensity<'a>(&'a FrameJudge, Order);
struct FramePmi<'a>(&'a FrameJudge, Order);

impl Judge for LegacyDefault<'_> {
    fn name(&self) -> String {
        "legacy default    ".to_owned()
    }
    fn score(&self, text: &str) -> Option<f64> {
        let value = self.0.mean_log_prob(text);
        value.is_finite().then_some(value)
    }
}

impl Judge for LegacyKnownOnly<'_> {
    fn name(&self) -> String {
        "legacy known-only ".to_owned()
    }
    fn score(&self, text: &str) -> Option<f64> {
        self.0.mean_log_prob_known_only(text)
    }
}

impl Judge for FrameDensity<'_> {
    fn name(&self) -> String {
        format!("{:<18}", format!("{} density*", self.1.label()))
    }
    fn score(&self, text: &str) -> Option<f64> {
        self.0.density_score(text, self.1)
    }
}

impl Judge for FramePmi<'_> {
    fn name(&self) -> String {
        format!("{:<18}", format!("{} PMI", self.1.label()))
    }
    fn score(&self, text: &str) -> Option<f64> {
        self.0.pmi(text, self.1)
    }
}

struct Row {
    rate: f64,
    margin: f64,
    counted: usize,
}

/// One length-matched forced-choice row. `suspect` is the side the judge is being
/// asked about; `reference` is held-out human text. The judge calls the
/// lower-scoring item "generated"; the rate is how often that is the suspect.
///
/// **0.50 means the judge cannot tell them apart.** When both sides are human,
/// 0.50 is the correct answer and anything approaching 1.000 is the judge
/// detecting something other than machine authorship.
fn forced_choice(judge: &dyn Judge, suspects: &[String], reference: &[&str]) -> Row {
    let mut cursor = 0usize;
    let mut correct = 0usize;
    let mut counted = 0usize;
    let mut margin_total = 0.0;
    for (trial, suspect) in suspects.iter().enumerate() {
        let real = item(reference, &mut cursor, Some(normalise(suspect).len()));
        let (Some(suspect_score), Some(real_score)) = (judge.score(suspect), judge.score(&real))
        else {
            continue;
        };
        counted += 1;
        margin_total += real_score - suspect_score;
        let picked = if suspect_score == real_score {
            trial % 2 == 0
        } else {
            suspect_score < real_score
        };
        if picked {
            correct += 1;
        }
    }
    Row {
        rate: correct as f64 / counted.max(1) as f64,
        margin: margin_total / counted.max(1) as f64,
        counted,
    }
}

/// Runs one suspect set through all three judges and prints the block.
fn report(label: &str, legacy: &LegacyReader, frames: &FrameJudge, suspects: &[String], reference: &[&str]) -> Vec<Row> {
    let words: usize = suspects.iter().map(|text| normalise(text).len()).sum();
    let mean_words = words as f64 / suspects.len().max(1) as f64;
    let unknown: f64 = suspects
        .iter()
        .map(|text| legacy.unknown_word_rate(text))
        .sum::<f64>()
        / suspects.len().max(1) as f64;
    let attested: f64 = suspects
        .iter()
        .map(|text| frames.attested_frame_rate(text))
        .sum::<f64>()
        / suspects.len().max(1) as f64;
    let closed: f64 = suspects
        .iter()
        .map(|text| frames.closed_class_rate(text))
        .sum::<f64>()
        / suspects.len().max(1) as f64;
    println!("\n  {label}");
    println!(
        "    {:.1} words/item  |  unknown to legacy vocabulary {:.1}%  |  \
         frame bigrams attested {:.1}%  |  closed-class {:.1}%",
        mean_words,
        unknown * 100.0,
        attested * 100.0,
        closed * 100.0
    );
    let judges: Vec<Box<dyn Judge>> = vec![
        Box::new(LegacyDefault(legacy)),
        Box::new(LegacyKnownOnly(legacy)),
        Box::new(FrameDensity(frames, Order::Bigram)),
        Box::new(FrameDensity(frames, Order::Trigram)),
        Box::new(FramePmi(frames, Order::Bigram)),
        Box::new(FramePmi(frames, Order::Trigram)),
    ];
    let mut rows = Vec::new();
    for judge in &judges {
        let row = forced_choice(judge.as_ref(), suspects, reference);
        println!(
            "    {}  rate {:.3}   margin {:+.4}   {} scoreable trials",
            judge.name(),
            row.rate,
            row.margin,
            row.counted
        );
        rows.push(row);
    }
    rows
}

/// Index into `report`'s output. Kept as named constants so an assertion reads
/// as the judge it is about.
///
/// `*` in the printed name marks a scorer that is **reported but never asserted
/// on**, because it failed a control of this file. Both legacy scorers and both
/// density scorers are in that category; only the two PMI rows are gated.
const LEGACY_DEFAULT: usize = 0;
#[allow(dead_code)]
const LEGACY_KNOWN_ONLY: usize = 1;
#[allow(dead_code)]
const FRAME_DENSITY_BIGRAM: usize = 2;
#[allow(dead_code)]
const FRAME_DENSITY_TRIGRAM: usize = 3;
const FRAME_PMI_BIGRAM: usize = 4;
const FRAME_PMI_TRIGRAM: usize = 5;

/// The scorers this file stands behind, and the only ones any control asserts on.
const GATED: [(usize, &str); 2] = [
    (FRAME_PMI_BIGRAM, "frame-bigram PMI"),
    (FRAME_PMI_TRIGRAM, "frame-trigram PMI"),
];

/// Two same-distribution halves of one corpus, taken by **interleaving** rather
/// than by cutting it in half.
///
/// This is not cosmetic. `chat-en-expanded-v1.txt` is ordered, so its first and
/// second halves differ in topic; splitting it in two and calling the result "two
/// samples of the same thing" put a 0.780 bias into Control 1 on the legacy
/// scorer that has nothing to do with the scorer. Interleaving removes it.
fn interleaved(lines: &[&'static str]) -> (Vec<&'static str>, Vec<&'static str>) {
    (
        lines.iter().step_by(2).copied().collect(),
        lines.iter().skip(1).step_by(2).copied().collect(),
    )
}


// ==========================================================================
// The bench: what is trained on what, and what is deliberately never trained on
// ==========================================================================
//
// The first cut of this file trained the frame judge on the same 400 lines of
// `chat-en-expanded-v1.txt` the broken judge uses, and it FAILED Control 2 at
// 1.000 on both outside provenances -- as hard as the scorer it was replacing.
// The measurement that explains it is printed on every row below: **frame
// bigrams attested**. A judge trained on 400 lines of one register had seen
// 95.4% of the frame bigrams in held-out text from that register and only
// **55.2%** of the frame bigrams in scraped SMS. Half of a second human
// corpus's syntax is, to that judge, unseen -- so "unseen" means "not mine",
// and the corpus-membership channel survives the closed alphabet intact.
//
// Closing the alphabet removes vocabulary as a channel. It does not remove
// corpus as a channel, and nothing about the score can: a model fitted to one
// corpus ranks that corpus above every other one. **That is a property of
// corpus-trained judges, not of a scoring rule**, and it is why B0-06's
// vocabulary-blind variant did not rescue the old reader either.
//
// So the training set is built the way the control demands rather than the way
// the corpus happens to be laid out:
//
// * **Two provenances are pooled for training** -- the project's hand-written
//   chat corpus and 4x as much external scraped chat (NPS Chat + UCI SMS ham).
//   A judge fitted to two registers cannot express "not my register" as cheaply
//   as one fitted to a single register.
// * **A third provenance is never trained on at all** -- `bigram_corpus.txt`.
//   It is the control, and it is the only text in this file the judge has no
//   claim on.
// * Every split is **interleaved, never cut in half**. `chat-en-expanded-v1.txt`
//   is ordered by topic and the external corpus is sorted alphabetically;
//   splitting either one in half produces two samples that are not of the same
//   distribution, which by itself put a 0.780 bias into Control 1.
//
// The control corpus is also, deliberately, the corpus the **shipping word-table
// codec is trained on**. So the codec's covers and the control text share a
// vocabulary and a register, and neither is in the judge's training set. If the
// judge passes the control and still flags the covers, the thing it is reacting
// to cannot be provenance -- there is no provenance difference left.

/// Deterministic shuffle. Used to build word salad out of real human sentences,
/// which is the mutant that isolates word ORDER from word CHOICE.
fn shuffled(text: &str, seed: &mut u64) -> String {
    let mut words = normalise(text);
    for index in (1..words.len()).rev() {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        let target = (*seed % (index as u64 + 1)) as usize;
        words.swap(index, target);
    }
    words.join(" ")
}

/// Length-matched suspect texts drawn from a corpus.
fn suspects_from(source: &[&str], count: usize, words: usize) -> Vec<String> {
    let mut cursor = 0usize;
    (0..count)
        .map(|_| item(source, &mut cursor, Some(words)))
        .collect()
}

/// One line in five is held out. Interleaved rather than contiguous.
const HELD_OUT_EVERY: usize = 5;

fn split_interleaved(lines: &[&'static str]) -> (Vec<&'static str>, Vec<&'static str>) {
    let mut training = Vec::new();
    let mut held_out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if index % HELD_OUT_EVERY == 0 {
            held_out.push(*line);
        } else {
            training.push(*line);
        }
    }
    (training, held_out)
}

/// Deterministically permute a line list.
///
/// The reference stream is read sequentially with wraparound and each trial
/// concatenates consecutive lines up to a word count, so **the order of the
/// reference matters**. Two held-out sets of very different sizes (136 project
/// lines, 325 external) cannot be zipped 1:1 without leaving a long uniform tail;
/// permuting the pool spreads both provenances evenly through it. Control 1
/// caught the first attempt at this, which left one reference stream entirely
/// project-corpus and the other entirely external and so turned the *bias* gate
/// into an accidental cross-provenance comparison.
fn permuted(lines: &[&'static str], mut seed: u64) -> Vec<&'static str> {
    let mut out = lines.to_vec();
    for index in (1..out.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        out.swap(index, (seed % (index as u64 + 1)) as usize);
    }
    out
}

struct Bench {
    /// The D-161 / B0-06 reader, trained on exactly the 400 lines it was trained
    /// on there, so its published numbers reproduce here byte for byte.
    legacy: LegacyReader,
    /// The judge. Trained on two pooled human provenances.
    frames: FrameJudge,
    /// Held-out human text of the **training** provenances. The reference side.
    reference: Vec<&'static str>,
    /// Second disjoint held-out stream, same provenances. Control 1 needs two.
    reference_b: Vec<&'static str>,
    /// The third provenance. **Never trained on.** The control suspect.
    control: Vec<&'static str>,
    training_lines: usize,
}

fn bench() -> Bench {
    let project_chat = lines_of(REAL_CHAT);
    let external_chat = lines_of(EXTERNAL_HUMAN_CHAT);
    let control = lines_of(OTHER_HUMAN_CHAT);
    assert!(!control.is_empty(), "the control corpus is empty");
    // If `fix/d116-cover-corpus` ever lands, `bigram_corpus.txt` becomes the very
    // text the external fixture holds, the control corpus lands in the training
    // pool, and this file silently becomes circular. Fail rather than measure a
    // judge against its own training data.
    assert_ne!(
        control.len(),
        external_chat.len(),
        "the control corpus and the external training corpus look identical -- \
         the control would be inside the training pool and every number below it is void"
    );

    let (project_train, project_held) = split_interleaved(&project_chat);
    let (external_train, external_held) = split_interleaved(&external_chat);
    let mut training = project_train.clone();
    training.extend_from_slice(&external_train);

    // The legacy reader keeps its ORIGINAL training split -- the first 400 lines
    // of the project corpus, contiguous -- because reproducing D-161's published
    // numbers is the point of keeping it.
    let legacy_training: Vec<&'static str> =
        project_chat.iter().take(READER_TRAINING_LINES).copied().collect();

    // Pool both held-out provenances, permute so neither reference stream is a
    // run of one of them, then split by parity. Both halves carry the same
    // provenance mix, which is what makes Control 1 a bias gate and not a
    // cross-provenance comparison in disguise.
    let mut pool = project_held.clone();
    pool.extend_from_slice(&external_held);
    let (reference, reference_b) = interleaved(&permuted(&pool, 0xd215_5911_7c0d_e001));

    Bench {
        legacy: LegacyReader::train(&legacy_training),
        frames: FrameJudge::train(&training),
        reference,
        reference_b,
        control,
        training_lines: training.len(),
    }
}

impl Bench {
    fn report(&self, label: &str, suspects: &[String], reference: &[&str]) -> Vec<Row> {
        report(label, &self.legacy, &self.frames, suspects, reference)
    }
}

/// Words per suspect item in every control. Chosen to sit inside the 40-61 word
/// band the two real cover paths occupy, so the controls and the measured rows
/// are the same size of text.
const CONTROL_WORDS: usize = 40;

// ==========================================================================
// CONTROL 1 - the bias gate the old harness had. Kept, and kept honest.
// ==========================================================================

/// Held-out human text against other held-out human text, **from the judge's own
/// training provenances**. Every judge should be at 0.500.
///
/// This is the D-161 gate `the_reader_cannot_tell_real_from_real`, restated. It is
/// retained because it is a real requirement -- a judge biased between two samples
/// of one distribution is unreadable -- but it is labelled for what it is: a
/// **bias** gate. It cannot detect corpus-membership detection, because both of its
/// sides are members. Reading it as a validity gate is D-215.
#[test]
fn control_1_the_judge_is_unbiased_within_its_own_corpus() {
    let bench = bench();
    println!("\n================ CONTROL 1: bias within the judge's own corpora ================");
    println!("  both sides are held-out lines of the two TRAINING provenances. 0.500 is unbiased.");
    println!("  THIS IS A BIAS GATE, NOT A VALIDITY GATE. See D-215.");

    let suspects = suspects_from(&bench.reference, trials(), CONTROL_WORDS);
    let rows = bench.report("held-out training-provenance vs held-out training-provenance", &suspects, &bench.reference_b);

    for (index, label) in GATED {
        assert!(
            (rows[index].rate - 0.5).abs() < 0.12,
            "{label} is biased within its own corpus ({:.3}); every number below it is an artifact",
            rows[index].rate
        );
    }
}


// ==========================================================================
// CONTROL 2 - THE ONE THAT MATTERS. Different provenance, still human.
// ==========================================================================

/// **The judges this file certifies as safe to quote a number from.**
///
/// **It is empty, and that is D-215's answer.** Measured 2026-08-04: no scorer in
/// this file passes `control_2_the_judge_does_not_flag_human_text_of_another_provenance`
/// in its two-sided, leave-one-provenance-out form. Not the legacy reader, not its
/// vocabulary-blind variant, not the closed-alphabet frame likelihood, not the
/// closed-alphabet PMI that fixes everything else.
///
/// Control 2 asserts that this list and the measurement **agree**. So:
///
/// * adding a scorer here without the control passing fails the test;
/// * improving a scorer until the control passes and *not* recording it here also
///   fails the test, with a message saying to record it;
/// * a scorer that was certified and regresses fails the test.
///
/// It is deliberately not possible to make this file green by tuning a scorer,
/// because Control 3 has to keep passing at the same time and it demands the
/// opposite behaviour.
const QUOTABLE: &[usize] = &[];

/// Printed names, indexed like `report`'s output.
const JUDGE_NAMES: [&str; 6] = [
    "legacy default",
    "legacy known-only",
    "frame-bigram density",
    "frame-trigram density",
    "frame-bigram PMI",
    "frame-trigram PMI",
];

/// How far from 0.500 a judge may sit on a human-versus-human comparison before
/// what it is measuring is provenance.
///
/// Fixed before measuring. 0.20 is wide on purpose -- it admits a rate anywhere in
/// `[0.30, 0.70]`, which is far more slack than any real instrument should need,
/// so that a failure cannot be argued away as a tight threshold. The measured
/// numbers are nowhere near it in either direction.
const PROVENANCE_TOLERANCE: f64 = 0.20;

/// **The control D-215 requires, in the only form that actually catches the
/// defect.**
///
/// Every suspect and every reference below is a sentence a person wrote. Nothing
/// generated a single word of any of it.
///
/// # Why it is leave-one-out and two-sided
///
/// The first form of this control gated one corpus in one direction against a
/// ceiling of 0.70, and **that gate is passable by accident**. Swapping the sides
/// of a forced choice turns a rate `r` into `1 − r`, so a judge that scores the
/// outside corpus *above* the reference passes a one-sided ceiling while being
/// exactly as provenance-sensitive as one that fails it. Measured: the frame-PMI
/// trigram scorer sat at **0.000** against `bigram_corpus.txt` -- it never once
/// called that human corpus generated -- and at **0.980** with the same two
/// corpora and the sides swapped. One number, dressed as a pass.
///
/// So: three human corpora, each in turn **excluded from training entirely** and
/// used as the control, judge trained on the other two, and the score is
/// `|rate − 0.500|` **regardless of sign**. A row at 0.000 is exactly as broken as
/// a row at 1.000. Both mean provenance.
///
/// The legacy scorers are re-trained on each pooled training set too, rather than
/// being left on their original 400 lines, so they are not failing for want of
/// data that the new judge was given.
///
/// # What this measured
///
/// All three rows, both new scorers, all six scorers: **nothing lands near 0.500.**
/// Two configurations flag out-of-provenance human writing at ~0.82; the third
/// scores it above the reference at 0.19/0.04. That is the finding, and it is why
/// `QUOTABLE` is empty.
#[test]
fn control_2_the_judge_does_not_flag_human_text_of_another_provenance() {
    let project = lines_of(REAL_CHAT);
    let external = lines_of(EXTERNAL_HUMAN_CHAT);
    let word_bank = lines_of(OTHER_HUMAN_CHAT);
    assert!(
        !project.is_empty() && !external.is_empty() && !word_bank.is_empty(),
        "a control corpus is empty"
    );
    // If `fix/d116-cover-corpus` ever lands, `bigram_corpus.txt` becomes the very
    // text the external fixture holds, two of the three "provenances" collapse
    // into one, and this control silently measures a judge against its own
    // training data. Fail rather than quietly halve the evidence.
    assert_ne!(
        word_bank.len(),
        external.len(),
        "two control corpora look identical -- they must be independent provenances"
    );

    let corpora: [(&str, &Vec<&'static str>); 3] = [
        ("project hand-written", &project),
        ("external scraped", &external),
        ("word-bank hand-written", &word_bank),
    ];

    println!("\n========= CONTROL 2: HUMAN text the judge did not train on =========");
    println!("  EVERY suspect and EVERY reference below was written by a person.");
    println!("  NOTHING here was generated. 0.500 is the only correct answer.");
    println!("  Each corpus in turn is EXCLUDED from training and used as the control;");
    println!("  the judge is trained on the other two. |rate-0.500| is the defect,");
    println!("  whichever way it points -- 0.000 is as broken as 1.000.");
    println!("  length-matched to {CONTROL_WORDS} words, {} trials per row.", trials());

    // Worst |rate - 0.5| each scorer reaches across the three configurations.
    let mut worst = [0.0f64; 6];
    for (excluded_index, (excluded_name, excluded_lines)) in corpora.iter().enumerate() {
        let mut training = Vec::new();
        let mut reference = Vec::new();
        for (index, (_, lines)) in corpora.iter().enumerate() {
            if index == excluded_index {
                continue;
            }
            let (train, held) = split_interleaved(lines);
            training.extend_from_slice(&train);
            reference.extend_from_slice(&held);
        }
        let reference = permuted(&reference, 0xd215_1000_c047_9011);
        let legacy = LegacyReader::train(&training);
        let frames = FrameJudge::train(&training);
        let suspects = suspects_from(excluded_lines, trials(), CONTROL_WORDS);
        let seen: f64 = suspects
            .iter()
            .map(|text| frames.attested_frame_rate(text))
            .sum::<f64>()
            / suspects.len() as f64;
        println!(
            "\n  EXCLUDED (= the control corpus): {excluded_name} -- {} lines, \
             trained on {} lines of the other two, {:.1}% of its frame bigrams ever seen",
            excluded_lines.len(),
            training.len(),
            seen * 100.0
        );
        let rows = report(
            "SUSPECT: the excluded human corpus. REFERENCE: held-out human text of the other two",
            &legacy,
            &frames,
            &suspects,
            &reference,
        );
        for (index, row) in rows.iter().enumerate() {
            worst[index] = worst[index].max((row.rate - 0.5).abs());
        }
    }

    println!("\n  WORST |rate - 0.500| over the three configurations. Both sides always human:");
    let mut passing = Vec::new();
    for (index, name) in JUDGE_NAMES.iter().enumerate() {
        let verdict = if worst[index] <= PROVENANCE_TOLERANCE {
            passing.push(index);
            "within tolerance"
        } else {
            "PROVENANCE DETECTOR"
        };
        println!("    {name:<24} {:.3}   {verdict}", worst[index]);
    }
    println!("\n  tolerance {PROVENANCE_TOLERANCE:.2} (a rate anywhere in [0.30, 0.70] passes).");
    println!("  Any scorer listed as PROVENANCE DETECTOR may not have a number quoted from it,");
    println!("  and that includes every scorer this project has ever quoted a number from.");

    // The gate: the file's recorded verdict and the live measurement must agree.
    // This is what makes the empty `QUOTABLE` list a claim rather than an absence.
    let certified: Vec<usize> = QUOTABLE.to_vec();
    assert_eq!(
        passing, certified,
        "D-215: the certified-quotable list and the measurement disagree.\n  \
         measured as passing: {:?}\n  recorded as quotable: {:?}\n  \
         If a scorer now passes, record it in QUOTABLE deliberately -- and check \
         Control 3 still passes for it, because a scorer tuned to pass this one \
         stops being able to flag word salad. If a scorer regressed, that is the bug.",
        passing
            .iter()
            .map(|index| JUDGE_NAMES[*index])
            .collect::<Vec<_>>(),
        certified
            .iter()
            .map(|index| JUDGE_NAMES[*index])
            .collect::<Vec<_>>(),
    );
    println!(
        "\n  CERTIFIED QUOTABLE: {:?} -- D-215 stands.",
        certified.iter().map(|index| JUDGE_NAMES[*index]).collect::<Vec<_>>()
    );
}

/// **Why the training pool has two provenances in it, measured rather than
/// asserted.**
///
/// Control 2 is passable only by a judge that has seen enough English to have an
/// opinion about a sentence it has never met. This walks the training-set size up
/// and prints, at each size, how much of the control corpus's syntax the judge has
/// actually seen and what it then says about it. The two columns move together,
/// and that is the whole argument for why the first cut of this file failed.
///
/// Reported, never asserted: it is a diagnostic that explains a design choice, and
/// pinning today's curve with a threshold would freeze the explanation into a
/// requirement.
#[test]
fn the_control_is_passable_only_above_a_training_scale() {
    let project_chat = lines_of(REAL_CHAT);
    let external_chat = lines_of(EXTERNAL_HUMAN_CHAT);
    let control = lines_of(OTHER_HUMAN_CHAT);
    let (project_train, project_held) = split_interleaved(&project_chat);
    let (external_train, external_held) = split_interleaved(&external_chat);
    let mut pool = project_held.clone();
    pool.extend_from_slice(&external_held);
    let reference = permuted(&pool, 0xd215_5911_7c0d_e001);

    println!("\n======= WHY THE POOL: control rate against training scale =======");
    println!("  suspect = the third-provenance control corpus. reference = held-out training text.");
    println!("  'seen' = share of the control corpus's frame bigrams the judge has ever read.");
    println!("  {:>28}  {:>6}  {:>9}  {:>9}", "training set", "lines", "seen", "rate(PMI2)");

    let mut pooled = project_train.clone();
    pooled.extend_from_slice(&external_train);
    let suspects = suspects_from(&control, trials(), CONTROL_WORDS);

    for (label, lines) in [
        ("project corpus only (D-161)", project_train.iter().take(400).copied().collect::<Vec<_>>()),
        ("project corpus, all", project_train.clone()),
        ("pooled, 1/4", pooled.iter().step_by(4).copied().collect::<Vec<_>>()),
        ("pooled, 1/2", pooled.iter().step_by(2).copied().collect::<Vec<_>>()),
        ("pooled, all (the judge)", pooled.clone()),
    ] {
        let judge = FrameJudge::train(&lines);
        let seen: f64 = suspects
            .iter()
            .map(|text| judge.attested_frame_rate(text))
            .sum::<f64>()
            / suspects.len() as f64;
        let row = forced_choice(&FramePmi(&judge, Order::Bigram), &suspects, &reference);
        println!(
            "  {label:>28}  {:>6}  {:>8.1}%  {:>9.3}",
            lines.len(),
            seen * 100.0,
            row.rate
        );
    }
    println!("  The first cut of this file was the top row. The control caught it.");
}

// ==========================================================================
// CONTROL 3 - the judge must be able to FAIL something.
// ==========================================================================

/// **A judge that cannot flag word salad is as broken as one that flags real
/// writing.** Control 2 alone is passed perfectly by a judge that says "human" to
/// everything, so it is worthless without this one.
///
/// Two salads, both constructed to defeat a vocabulary detector:
///
/// * **shuffled human** - a held-out human sentence with its words permuted. Same
///   words, same unigram distribution, same corpus. **Only the order changed.**
///   This is the cleanest possible isolation of syntax.
/// * **vocabulary-matched random** - uniform draws from the judge's *own* training
///   vocabulary. Every word is one the judge has read.
///
/// The second one is not redundant. It is what killed this file's first scorer:
/// a plain frame likelihood called uniform random words **more human than real
/// text** (rate 0.045), because random draws from a chat vocabulary are 91%
/// open-class and `<OPEN> <OPEN>` is the commonest transition there is.
#[test]
fn control_3_the_judge_flags_word_salad() {
    let bench = bench();
    println!("\n============= CONTROL 3: the judge must be able to fail =============");
    println!("  1.000 = spotted every time. A judge that cannot reach it here measures nothing.");

    let mut seed = 0xd215_c047_9013_5eedu64;
    let salad: Vec<String> = suspects_from(&bench.reference, trials(), CONTROL_WORDS)
        .iter()
        .map(|text| shuffled(text, &mut seed))
        .collect();
    let rows_shuffled = bench.report(
        "SUSPECT: held-out HUMAN text with its words shuffled (order destroyed, words identical)",
        &salad,
        &bench.reference_b,
    );

    let project_chat = lines_of(REAL_CHAT);
    let (project_train, _) = split_interleaved(&project_chat);
    let vocabulary: Vec<String> = project_train
        .iter()
        .flat_map(|line| normalise(line))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut seed = 0xd215_5a1a_d000_0002u64;
    let random: Vec<String> = (0..trials())
        .map(|_| {
            (0..CONTROL_WORDS)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    vocabulary[(seed % vocabulary.len() as u64) as usize].clone()
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    let rows_random = bench.report(
        "SUSPECT: uniform draws from the judge's OWN training vocabulary",
        &random,
        &bench.reference_b,
    );

    const SALAD_FLOOR: f64 = 0.85;
    for (rows, what) in [
        (&rows_shuffled, "shuffled human text"),
        (&rows_random, "vocabulary-matched random words"),
    ] {
        for (index, label) in GATED {
            assert!(
                rows[index].rate >= SALAD_FLOOR,
                "{label} flags {what} at only {:.3} (floor {SALAD_FLOOR:.2}). \
                 A judge that cannot fail on word salad is decoration.",
                rows[index].rate
            );
        }
    }
}

// ==========================================================================
// THE ROWS
// ==========================================================================

/// **The re-measurement.** Same held-out protocol, same payload seed, same trial
/// count and the same length matching as `tasklogs/B0-06.md` -- only the judge and
/// the reference corpus changed, and both changes are what Control 2 demanded.
///
/// Row 1 is the shipping word-table codec, generated in process from
/// `stego::bigram` so it is what actually renders covers on this branch today.
/// Row 2 is B0-06's arithmetic-coded model covers, read from the file that lane
/// dumped (`OSL_COVER_DUMP`); without it the row prints NOT MEASURED rather than
/// inventing a number.
///
/// **Row 1's provenance is controlled and this is the tightest thing in the file.**
/// The word-table codec is a bigram sampler over `stego/src/bigram_corpus.txt` --
/// which is Control 2's corpus. So the covers and the control text share a
/// vocabulary, a register and a source, and *neither* is in the judge's training
/// pool. Control 2 having passed means that shared provenance costs a text
/// nothing. Anything the judge says about Row 1 is therefore about how the words
/// are ordered, which is the only thing left that differs.
#[test]
fn the_three_rows_remeasured() {
    let bench = bench();
    println!("\n==================== THE ROWS, RE-MEASURED ====================");
    println!("  *** NOT QUOTABLE. QUOTABLE is {:?} -- see Control 2. ***", QUOTABLE);
    println!("  Every rate below comes from a scorer that Control 2 classifies as a provenance");
    println!("  detector. These rows are DIRECTIONAL EVIDENCE about where the difference lies,");
    println!("  not measurements of how often a person would look twice. Row 1b is the only one");
    println!("  in which provenance is controlled on both sides.");
    println!(
        "  judge: closed-class frame PMI, {} training lines, two pooled provenances",
        bench.training_lines
    );
    println!(
        "  reference: {} held-out human lines of those provenances",
        bench.reference.len()
    );
    println!("  length-matched, {} trials per row, payload seed 0x0517d161c0defa11", trials());

    let mut payloads = Payloads(0x0517_d161_c0de_fa11);
    let word_table: Vec<String> = (0..trials()).map(|_| payloads.next_cover()).collect();
    bench.report(
        "ROW 1 -- SHIPPING word-table codec (stego::bigram, what every friend sends today)",
        &word_table,
        &bench.reference,
    );
    println!("\n    ROW 1b -- the same covers against their OWN provenance (Control 2's corpus).");
    println!("    Reference and suspect now share vocabulary, register and source.");
    bench.report(
        "ROW 1b -- word-table covers vs the human corpus the codec is trained on",
        &word_table,
        &bench.control,
    );
    println!("\n    first three word-table covers:");
    for (index, cover) in word_table.iter().take(3).enumerate() {
        println!("      {}. {cover}", index + 1);
    }

    match std::env::var("OSL_COVER_DUMP")
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
    {
        Some(dump) => {
            let mut covers: Vec<String> = dump
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect();
            covers.truncate(trials());
            println!("\n    ({} model-written covers read from OSL_COVER_DUMP)", covers.len());
            bench.report(
                "ROW 2 -- B0-06 arithmetic-coded MODEL covers (feat/b006-arithmetic-cover)",
                &covers,
                &bench.reference,
            );
            println!("\n    ROW 2b -- the same model covers against the third-provenance corpus,");
            println!("    so Row 2 can be read against Row 1b on the same reference.");
            bench.report(
                "ROW 2b -- model covers vs stego/src/bigram_corpus.txt",
                &covers,
                &bench.control,
            );
            println!("\n    first three model covers:");
            for (index, cover) in covers.iter().take(3).enumerate() {
                println!("      {}. {cover}", index + 1);
            }
        }
        None => println!(
            "\n  ROW 2 -- NOT MEASURED. Set OSL_COVER_DUMP to runlogs/b006-model-covers.txt."
        ),
    }
}

/// **The D-161 row, reproduced exactly, so the two protocols can be compared.**
///
/// Legacy reader, its original contiguous 400-line training split, its original
/// held-out reference, its original seed and length matching. This must print
/// `rate 1.000, margin +0.8306` for the word table -- the number in
/// `tasklogs/B0-06.md` and in D-161. If it does not, this file's re-measurement is
/// not measuring the same thing the old one measured and nothing below it can be
/// compared to anything above it.
#[test]
fn the_old_protocol_still_reproduces_its_published_numbers() {
    let lines = lines_of(REAL_CHAT);
    let (training, held_out) = lines.split_at(READER_TRAINING_LINES);
    let legacy = LegacyReader::train(training);
    let frames = FrameJudge::train(training);

    println!("\n=========== D-161 / B0-06 PROTOCOL, REPRODUCED ===========");
    println!("  legacy reader, 400 contiguous training lines, held-out reference, same seed.");
    let mut payloads = Payloads(0x0517_d161_c0de_fa11);
    let covers: Vec<String> = (0..trials()).map(|_| payloads.next_cover()).collect();
    let rows = report(
        "declined path (word table), length-matched -- D-161 published 1.000 / +0.8306",
        &legacy,
        &frames,
        &covers,
        held_out,
    );
    if trials() == 200 {
        assert!(
            (rows[LEGACY_DEFAULT].rate - 1.000).abs() < 1e-9
                && (rows[LEGACY_DEFAULT].margin - 0.8306).abs() < 5e-4,
            "the old protocol no longer reproduces its published number \
             (rate {:.3}, margin {:+.4}); the re-measurement is not comparable",
            rows[LEGACY_DEFAULT].rate,
            rows[LEGACY_DEFAULT].margin
        );
        println!("  reproduced: rate 1.000, margin +0.8306 -- byte-for-byte the published row.");
    }
}

// ==========================================================================
// WHAT THE JUDGE CANNOT SEE
// ==========================================================================

/// **Stated as a measurement, not as a caveat.**
///
/// A human flagged D-165's covers 20/20 and forced-choice 20/20, and B0-06 named
/// what a person reacts to in the model-written covers: they end **mid-clause**,
/// and the **register is a chatbot's**, not a friend's. Neither is a word-order
/// property inside a two- or three-word window, so neither is visible to anything
/// in this file.
///
/// Rather than assert that in prose, this measures the first one on text where the
/// ground truth is not in doubt: held-out HUMAN sentences, cut before their last
/// clause, against uncut human sentences. A person spots that instantly. Whatever
/// the judge scores here is its entire sensitivity to truncation, with fluency,
/// vocabulary and provenance held human on both sides.
///
/// Register is **not** measured here and is not faked with a number: there is no
/// labelled sample of chatbot-voiced versus friend-voiced chat on this branch. It
/// is named in the tasklog as unmeasured.
///
/// Reported, never asserted. Pinning a blind spot with an assertion would make it
/// a requirement, and the point is that it is a limit to be reported alongside
/// every number this file produces.
#[test]
fn what_this_judge_cannot_see() {
    let bench = bench();
    println!("\n=============== WHAT THIS JUDGE CANNOT SEE ===============");
    println!("  both sides HUMAN. the suspect side is truncated mid-clause, which is one of the");
    println!("  two things a person notices about B0-06's covers. 0.500 = the judge is blind.");

    let truncated: Vec<String> = suspects_from(&bench.reference, trials(), CONTROL_WORDS)
        .iter()
        .map(|text| {
            let words = normalise(text);
            let keep = (words.len() * 7 / 10).max(MIN_TRANSITIONS + 1);
            words[..keep.min(words.len())].join(" ")
        })
        .collect();
    bench.report(
        "SUSPECT: human text truncated mid-clause",
        &truncated,
        &bench.reference_b,
    );
    println!("\n  Read that rate as the judge's sensitivity to truncation ALONE. The second");
    println!("  thing a human notices -- chatbot register -- is not measured anywhere in this");
    println!("  file, because no labelled sample of it exists on this branch. An n-gram over a");
    println!("  closed function-word alphabet cannot see topic, register, or where a sentence");
    println!("  stops. D-165's 20/20 was a HUMAN result and nothing here substitutes for it.");
}
