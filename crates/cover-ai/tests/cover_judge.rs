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
//! # The judge added here: a closed-alphabet syntactic-frame model
//!
//! The fix is not more corpus. It is to **close the alphabet**, so that "this text
//! came from somewhere else" cannot be expressed at all.
//!
//! Every word is mapped to one of ~200 symbols:
//!
//! * if it is an English **closed-class** word — determiner, pronoun, preposition,
//!   auxiliary, modal, conjunction, negator, wh-word, or a common contraction of
//!   those — it maps to **itself**;
//! * otherwise it maps to a single symbol `<OPEN>`.
//!
//! Closed-class inventories are a property of the **language**, not of a corpus:
//! `the`, `of`, `is`, `not` are the same words in a hand-written fixture, in
//! scraped SMS, and in text a transformer wrote. So a model over this alphabet has
//! **no channel through which provenance can leak** — a word it has never seen
//! before is not a hole in its vocabulary, it is `<OPEN>`, which it has seen tens
//! of thousands of times.
//!
//! What survives that mapping is **syntactic frame**: `a <OPEN>`, `<OPEN> of the`,
//! `i do not`. That is exactly what the shipping word-table codec destroys — it
//! draws high-frequency chat words independently, producing frames like `a you`,
//! `of not`, `in the of` that no English speaker writes — and exactly what fluent
//! text preserves regardless of who wrote it or what it is about.
//!
//! The model is trained on **the same 400 lines** the broken judge trains on, so
//! the comparison below isolates the *representation* and not the amount of data.
//!
//! ## Parameters, fixed before measuring
//!
//! Interpolation weights are stated here and were **not** adjusted after seeing a
//! result. A judge tuned until it agrees with what we already believe is not a
//! judge. `BIGRAM_LAMBDA = 0.7`; trigram weights `0.5 / 0.3 / 0.2`. Both orders are
//! reported, and both must pass every control — a parameter that only works at one
//! setting would itself be the finding.
//!
//! There is **no threshold on the score**. Every number here comes from a
//! forced-choice trial between two texts, so the judge only ever has to order a
//! pair. There is nothing to calibrate and therefore nothing to tune.
//!
//! # The controls, and they run by default
//!
//! | control | requirement |
//! |---|---|
//! | held-out text from the judge's **own** corpus | must **not** be flagged (≈0.50) |
//! | human chat of a **different provenance** (`stego/src/bigram_corpus.txt`) | must **not** be flagged |
//! | **external, scraped** human chat (NPS Chat + UCI SMS ham) | must **not** be flagged |
//! | word salad — human words, shuffled | **must** be flagged |
//! | word salad — uniform draws from the judge's own vocabulary | **must** be flagged |
//!
//! The last two are not decoration. A judge that cannot fail on obvious word salad
//! is as broken as one that fires on real human writing, and the second control
//! alone can be passed by a judge that says "human" to everything.
//!
//! # The old scorer is kept, not deleted
//!
//! `LegacyReader` below is the D-161/B0-06 reader **verbatim**, including
//! `mean_log_prob_known_only`. Every control is run through it as well, so its
//! failure is visible in the same output as the new judge's result rather than
//! having to be taken on trust from a prose defect entry. NEVER REMOVE A SPEC.
//!
//! # Running the rows
//!
//! ```text
//! cargo test -p cover-ai --test cover_judge -- --nocapture --test-threads=1
//!
//! # the B0-06 model-written row, from the covers that lane dumped:
//! OSL_COVER_DUMP=/home/liamw/osl-plan/plan-test/runlogs/b006-model-covers.txt \
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

    /// Mean log-probability per transition over the frame alphabet, or `None`
    /// when the text is too short to say anything about.
    fn score(&self, text: &str, order: Order) -> Option<f64> {
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
struct Frame<'a>(&'a FrameJudge, Order);

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

impl Judge for Frame<'_> {
    fn name(&self) -> String {
        format!("{:<18}", self.1.label())
    }
    fn score(&self, text: &str) -> Option<f64> {
        self.0.score(text, self.1)
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
        Box::new(Frame(frames, Order::Bigram)),
        Box::new(Frame(frames, Order::Trigram)),
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
const LEGACY_DEFAULT: usize = 0;
const LEGACY_KNOWN_ONLY: usize = 1;
const FRAME_BIGRAM: usize = 2;
const FRAME_TRIGRAM: usize = 3;

fn split_corpus() -> (Vec<&'static str>, Vec<&'static str>) {
    let lines = lines_of(REAL_CHAT);
    assert!(
        lines.len() > READER_TRAINING_LINES + 50,
        "the judge's corpus is too small to hold anything out"
    );
    let (training, held_out) = lines.split_at(READER_TRAINING_LINES);
    (training.to_vec(), held_out.to_vec())
}

fn build() -> (LegacyReader, FrameJudge, Vec<&'static str>) {
    let (training, held_out) = split_corpus();
    (
        LegacyReader::train(&training),
        FrameJudge::train(&training),
        held_out,
    )
}

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

/// Words per suspect item in every control. Chosen to sit inside the 40–61 word
/// band the two real cover paths occupy, so the controls and the measured rows
/// are the same size of text.
const CONTROL_WORDS: usize = 40;

// ==========================================================================
// CONTROL 1 — the bias gate the old harness had. Kept, and kept honest.
// ==========================================================================

/// Held-out human text against other held-out human text, **from the same
/// corpus**. Both judges should be at 0.50.
///
/// This is the D-161 gate `the_reader_cannot_tell_real_from_real`, restated. It
/// is retained because it is a real requirement — a judge biased between two
/// samples of one distribution is unreadable — but it is labelled for what it is:
/// a **bias** gate. It cannot detect corpus-membership detection, because both of
/// its sides are members. Reading it as a validity gate is D-215.
#[test]
fn control_1_the_judge_is_unbiased_within_its_own_corpus() {
    let (legacy, frames, held_out) = build();
    println!("\n================ CONTROL 1: bias within the judge's own corpus ================");
    println!("  both sides are held-out lines of chat-en-expanded-v1.txt. 0.500 is unbiased.");
    println!("  THIS IS A BIAS GATE, NOT A VALIDITY GATE. See D-215.");

    let half = held_out.len() / 2;
    let (left, right) = held_out.split_at(half);
    let suspects = suspects_from(left, trials(), CONTROL_WORDS);
    let rows = report("held-out A vs held-out A", &legacy, &frames, &suspects, right);

    for (index, label) in [
        (LEGACY_DEFAULT, "legacy default"),
        (LEGACY_KNOWN_ONLY, "legacy known-only"),
        (FRAME_BIGRAM, "frame bigram"),
        (FRAME_TRIGRAM, "frame trigram"),
    ] {
        assert!(
            (rows[index].rate - 0.5).abs() < 0.12,
            "{label} is biased within its own corpus ({:.3}); every number below it is an artifact",
            rows[index].rate
        );
    }
}

// ==========================================================================
// CONTROL 2 — THE ONE THAT MATTERS. Different provenance, still human.
// ==========================================================================

/// **The control D-215 requires, and the reason this file exists.**
///
/// Every suspect here is a sentence a person wrote. Nothing generated a single
/// word of it. Two independent outside provenances are used:
///
/// 1. `stego/src/bigram_corpus.txt` — hand-written texting-register English.
/// 2. `fixtures/external-human-chat-v1.txt` — real scraped chat (NPS Chat + UCI
///    SMS ham). Nobody on this project wrote it, so it closes the one loophole
///    corpus 1 leaves open: that both hand-written corpora share an author.
///
/// A judge that detects **generation** must sit near 0.500 on both. A judge that
/// detects **membership of `chat-en-expanded-v1.txt`** sits near 1.000, and every
/// number it has ever produced means "this text came from somewhere else".
///
/// **This test fails loudly if the judge regresses to corpus-membership
/// detection.** That is its entire job, and it is asserted on the new judge only:
/// the legacy scorers are run beside it and are *expected* to fail, so their
/// numbers are printed and not asserted. Deleting or loosening this assertion
/// re-opens D-215.
#[test]
fn control_2_the_judge_does_not_flag_human_text_of_another_provenance() {
    let (legacy, frames, held_out) = build();
    let hand_written = lines_of(OTHER_HUMAN_CHAT);
    let external = lines_of(EXTERNAL_HUMAN_CHAT);

    assert!(!hand_written.is_empty() && !external.is_empty(), "a control corpus is empty");
    // If `fix/d116-cover-corpus` ever lands, `bigram_corpus.txt` becomes the very
    // text this fixture holds and the two controls silently collapse into one.
    // Fail rather than quietly halve the evidence.
    assert_ne!(
        hand_written.len(),
        external.len(),
        "the two control corpora look identical -- they must be independent provenances"
    );

    println!("\n========= CONTROL 2: HUMAN text the judge did not train on =========");
    println!("  every suspect below was written by a person. NOTHING here was generated.");
    println!("  0.500 = the judge detects generation.  1.000 = it detects corpus membership.");
    println!("  length-matched to {CONTROL_WORDS} words, {} trials per row.", trials());

    let corpus_1 = suspects_from(&hand_written, trials(), CONTROL_WORDS);
    let rows_1 = report(
        "SUSPECT: hand-written texting chat (stego/src/bigram_corpus.txt)",
        &legacy,
        &frames,
        &corpus_1,
        &held_out,
    );

    let corpus_2 = suspects_from(&external, trials(), CONTROL_WORDS);
    let rows_2 = report(
        "SUSPECT: external scraped chat (NPS Chat + UCI SMS ham)",
        &legacy,
        &frames,
        &corpus_2,
        &held_out,
    );

    println!(
        "\n  LEGACY VERDICT (not asserted, reproduced so D-215 can be re-observed on demand):"
    );
    println!(
        "    the legacy default scorer calls hand-written human chat 'generated' at {:.3}",
        rows_1[LEGACY_DEFAULT].rate
    );
    println!(
        "    and external scraped human chat at {:.3}. Both sides are human.",
        rows_2[LEGACY_DEFAULT].rate
    );

    // The gate. 0.70 was chosen before any measurement as "meaningfully better
    // than the 1.000 that withdrew the instrument", and is NOT tightened to
    // whatever today's number happens to be -- pinning a threshold to a measured
    // value turns a control into decoration.
    const HUMAN_CEILING: f64 = 0.70;
    for (rows, corpus) in [(&rows_1, "hand-written outside chat"), (&rows_2, "external scraped chat")] {
        for (index, label) in [(FRAME_BIGRAM, "frame bigram"), (FRAME_TRIGRAM, "frame trigram")] {
            assert!(
                rows[index].rate <= HUMAN_CEILING,
                "D-215 REGRESSION: {label} calls {corpus} 'generated' at {:.3} (ceiling {HUMAN_CEILING:.2}). \
                 The judge is detecting provenance, not generation, and no number it produces may be quoted.",
                rows[index].rate
            );
        }
    }
}

// ==========================================================================
// CONTROL 3 — the judge must be able to FAIL something.
// ==========================================================================

/// **A judge that cannot flag word salad is as broken as one that flags real
/// writing.** Control 2 alone is passed perfectly by a judge that says "human" to
/// everything, so it is worthless without this.
///
/// Two salads, both deliberately constructed to defeat a vocabulary detector:
///
/// * **shuffled human** — a held-out human sentence with its words permuted. Same
///   words, same unigram distribution, same corpus. **Only the order changed.**
///   This is the cleanest possible isolation of syntax.
/// * **vocabulary-matched random** — uniform draws from the judge's *own*
///   training vocabulary. Every word is one the judge has read.
#[test]
fn control_3_the_judge_flags_word_salad() {
    let (legacy, frames, held_out) = build();
    println!("\n============= CONTROL 3: the judge must be able to fail =============");
    println!("  1.000 = spotted every time. A judge that cannot reach it here measures nothing.");

    let half = held_out.len() / 2;
    let (source, reference) = held_out.split_at(half);

    let mut seed = 0xd215_c047_9013_5eedu64;
    let salad: Vec<String> = suspects_from(source, trials(), CONTROL_WORDS)
        .iter()
        .map(|text| shuffled(text, &mut seed))
        .collect();
    let rows_shuffled = report(
        "SUSPECT: held-out HUMAN text with its words shuffled (order destroyed, words identical)",
        &legacy,
        &frames,
        &salad,
        reference,
    );

    let (training, _) = split_corpus();
    let mut vocabulary: Vec<String> = training
        .iter()
        .flat_map(|line| normalise(line))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    vocabulary.sort();
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
    let rows_random = report(
        "SUSPECT: uniform draws from the judge's OWN training vocabulary",
        &legacy,
        &frames,
        &random,
        reference,
    );

    const SALAD_FLOOR: f64 = 0.85;
    for (rows, what) in [
        (&rows_shuffled, "shuffled human text"),
        (&rows_random, "vocabulary-matched random words"),
    ] {
        for (index, label) in [(FRAME_BIGRAM, "frame bigram"), (FRAME_TRIGRAM, "frame trigram")] {
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
// THE THREE ROWS
// ==========================================================================

/// **The re-measurement.** Same held-out split, same seed, same length-matched
/// protocol, same trial count as `tasklogs/B0-06.md` — only the judge changed.
///
/// Row 1 is the shipping word-table codec, generated in process from
/// `stego::bigram` so it is what actually renders covers today. Row 2 is B0-06's
/// arithmetic-coded model covers, read from the file that lane dumped
/// (`OSL_COVER_DUMP`); it prints NOT MEASURED without it rather than inventing a
/// number.
#[test]
fn the_three_rows_remeasured() {
    let (legacy, frames, held_out) = build();
    println!("\n==================== THE ROWS, RE-MEASURED ====================");
    println!("  corpus: chat-en-expanded-v1.txt, lines 401.. held out ({} lines)", held_out.len());
    println!("  length-matched, {} trials per row, payload seed 0x0517d161c0defa11", trials());

    let mut payloads = Payloads(0x0517_d161_c0de_fa11);
    let word_table: Vec<String> = (0..trials()).map(|_| payloads.next_cover()).collect();
    report(
        "ROW 1 -- SHIPPING word-table codec (stego::bigram, what every friend sends today)",
        &legacy,
        &frames,
        &word_table,
        &held_out,
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
            report(
                "ROW 2 -- B0-06 arithmetic-coded MODEL covers (feat/b006-arithmetic-cover)",
                &legacy,
                &frames,
                &covers,
                &held_out,
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

// ==========================================================================
// WHAT THE JUDGE CANNOT SEE
// ==========================================================================

/// **Stated as a measurement, not as a caveat.**
///
/// A human flagged 20/20 on D-165's covers. Two of the three things a human
/// notices about B0-06's model covers are invisible to any n-gram statistic, and
/// rather than assert that in prose this test demonstrates it on human text where
/// the ground truth is not in doubt.
///
/// * **Mid-clause truncation.** Held-out HUMAN sentences, cut before their last
///   clause. A person spots this instantly. If the judge does not, then B0-06's
///   "it ends mid-clause" defect is outside its resolving power.
/// * **Register.** Not measurable here without a labelled sample, and it is named
///   in the tasklog rather than faked with a number.
///
/// Reported, never asserted. Pinning a blind spot with an assertion would make it
/// a requirement, and the point is that it is a limit to be reported alongside
/// every number this file produces.
#[test]
fn what_this_judge_cannot_see() {
    let (legacy, frames, held_out) = build();
    println!("\n=============== WHAT THIS JUDGE CANNOT SEE ===============");
    println!("  both sides HUMAN. the suspect side is truncated mid-clause, which is what");
    println!("  a person notices about B0-06's covers. 0.500 = the judge is blind to it.");

    let half = held_out.len() / 2;
    let (source, reference) = held_out.split_at(half);
    let truncated: Vec<String> = suspects_from(source, trials(), CONTROL_WORDS)
        .iter()
        .map(|text| {
            let words = normalise(text);
            let keep = (words.len() * 7 / 10).max(MIN_TRANSITIONS + 1);
            words[..keep.min(words.len())].join(" ")
        })
        .collect();
    report(
        "SUSPECT: human text truncated mid-clause",
        &legacy,
        &frames,
        &truncated,
        reference,
    );
    println!(
        "\n  Whatever the rate above, it is measured on text a person WROTE. Read it as the"
    );
    println!("  judge's sensitivity to truncation alone, with fluency and register held human.");
}
