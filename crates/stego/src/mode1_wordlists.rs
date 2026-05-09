//! Wordlists for Mode 1 stego templates.
//!
//! Each list has exactly 256 entries (8 bits per slot fill). Words
//! are lowercase, single-token (no spaces, no punctuation), and
//! chosen to be common enough that strings of them read as
//! plausible chat. The selection is *not* curated for fluency
//! beyond "doesn't read like a CSV" — Mode 1 in v1 alpha is the
//! prototype; v1 stable bumps to Mode 2 (Markov) or Mode 3 (LLM)
//! for actual stealth against scanners.
//!
//! Constraint: no word in any list may also appear as a fixed
//! token in any [`crate::mode1_templates::TEMPLATES`] template.
//! That would create decode ambiguity (the parser couldn't tell
//! whether the token came from a slot or the template skeleton).
//! The compile-time test
//! [`crate::mode1::tests::wordlists_disjoint_from_template_skeletons`]
//! enforces this invariant.

pub const NOUN_COUNT: usize = 256;
pub const ADJ_COUNT: usize = 256;

pub static NOUNS: [&str; NOUN_COUNT] = [
    "apple", "river", "mountain", "engine", "garden", "lantern", "bridge", "horizon",
    "ocean", "cabin", "pencil", "library", "harbor", "tractor", "blanket", "valley",
    "rocket", "donkey", "sandal", "compass", "trumpet", "lobster", "cucumber", "tunnel",
    "diamond", "feather", "glacier", "bracelet", "trolley", "ladder", "compass2", "puzzle",
    "kettle", "saddle", "cricket", "ribbon", "mailbox", "sapphire", "telescope", "campfire",
    "biscuit", "toaster", "pepper", "sweater", "kitten", "umbrella", "barrel", "elephant",
    "iceberg", "cucumber2", "porter", "violin", "raincoat", "cushion", "boulder", "ferret",
    "blanket2", "sandwich", "pottery", "fountain", "cauldron", "monocle", "passport", "platform",
    "yogurt", "cobra", "trampoline", "moccasin", "harvester", "panda", "ostrich", "owl",
    "vulture", "crayon", "garage", "lighthouse", "magnet", "muffin", "raccoon", "shovel",
    "sleigh", "thunder", "trinket", "victory", "wagon", "whistle", "wreath", "yacht",
    "zebra", "anchor", "brigade", "carousel", "deluge", "echo", "fjord", "gondola",
    "hamlet", "igloo", "jubilee", "kayak", "lagoon", "mosaic", "nectarine", "outpost",
    "parrot", "quartz", "ranger", "satchel", "tortoise", "underbrush", "vintage", "wagon2",
    "xylophone", "yardstick", "zenith", "arsenal", "balcony", "cottage", "drawbridge", "elixir",
    "frigate", "gargoyle", "hangar", "iceberg2", "jamboree", "kerosene", "lariat", "marquee",
    "narrative", "obelisk", "pavilion", "quagmire", "regiment", "scaffold", "trellis", "ukulele",
    "vortex", "windmill", "yardarm", "zodiac", "aviary", "ballad", "cavern", "dirigible",
    "exodus", "fortress", "grotto", "harness", "incubator", "jackpot", "kindling", "labyrinth",
    "menagerie", "nautilus", "outpouring", "pageant", "quintet", "rapier", "skiff", "tapestry",
    "umbra", "viaduct", "wagonwheel", "xenolith", "yeoman", "zeppelin", "abacus", "bayonet",
    "carousel2", "dollop", "embers", "filigree", "geode", "halberd", "icicle", "javelin",
    "kazoo", "lavender", "mango", "nutmeg", "oboe", "papyrus", "quiver", "ramekin",
    "stiletto", "tureen", "umbel", "veneer", "watchtower", "xebec", "yurt", "zucchini",
    "almond", "boulevard", "carpet", "dahlia", "embroidery", "fennel", "ginger", "hassock",
    "isthmus", "jonquil", "kestrel", "lichen", "moccasin2", "nougat", "octopus", "petunia",
    "quetzal", "raisin", "shawl", "thistle", "umbra2", "vellum", "willow", "xanthan",
    "yarrow", "zinnia", "amphora", "barometer", "calliope", "decanter", "ewer", "flotilla",
    "guitar", "hourglass", "icetray", "jardin", "kayak2", "lacquer", "mantilla", "nimbus",
    "orchid", "pendant", "quintessence", "regalia", "samovar", "talisman", "uvula", "vestibule",
    "wickerwork", "xerography", "yardage", "zither", "alpaca", "bonsai", "chalet", "doublet",
    "epiphany", "filly", "gauntlet", "halo", "icehouse", "jubilation", "kabuki", "lectern",
];

pub static ADJECTIVES: [&str; ADJ_COUNT] = [
    "tiny", "loud", "calm", "swift", "rough", "smooth", "bright", "dim",
    "fluffy", "wooden", "iron", "stone", "silver", "golden", "ancient", "modern",
    "rural", "urban", "frozen", "burning", "lively", "sleepy", "tidy", "messy",
    "polite", "gruff", "sharp", "blunt", "fragrant", "musty", "sticky", "slick",
    "warm", "icy", "muggy", "breezy", "stormy", "sunny", "foggy", "starry",
    "lazy", "eager", "shy", "bold", "timid", "fierce", "gentle", "sturdy",
    "brittle", "flexible", "rigid", "lazy2", "perky", "gloomy", "cheerful", "moody",
    "salty", "sweet", "bitter", "sour", "spicy", "bland", "hearty", "frail",
    "rusty", "polished", "weathered", "faded", "vibrant", "muted", "neon", "pastel",
    "drab", "ornate", "plain", "fancy", "simple", "complex", "regal", "humble",
    "lofty", "sunken", "perched", "grounded", "drifting", "anchored", "floating", "rooted",
    "thawing", "boiling", "simmering", "freezing", "melting", "evaporating", "condensing", "crystallizing",
    "scaly", "feathery", "leathery", "velvety", "satiny", "wooly", "hairy", "smooth2",
    "knotty", "tangled", "neat", "crumpled", "starched", "wrinkled", "creased", "pressed",
    "blooming", "wilting", "budding", "ripening", "rotting", "sprouting", "drying", "soaking",
    "humming", "buzzing", "rustling", "creaking", "whispering", "shouting", "roaring", "purring",
    "tribal", "civic", "rustic", "alpine", "coastal", "tropical", "arctic", "temperate",
    "muted2", "shimmering", "glittering", "matte", "glossy", "lustrous", "dull", "sparkling",
    "crystal", "muddy", "clear", "cloudy", "transparent", "opaque", "tinted", "stained",
    "balmy", "torrid", "chilly", "tepid", "scalding", "blistering", "mild", "intense",
    "scant", "ample", "meager", "abundant", "generous", "stingy", "lavish", "spartan",
    "mellow", "brash", "subdued", "boisterous", "raucous", "tranquil", "peaceful", "frenzied",
    "stalwart", "brittle2", "sturdy2", "fragile", "tough", "supple", "rigid2", "limber",
    "regal2", "common", "noble", "rustic2", "vulgar", "refined", "polished2", "crude",
    "oblong", "round", "square", "oval", "triangular", "spherical", "cubic", "conical",
    "tilted", "level", "slanted", "vertical", "horizontal", "diagonal", "askew", "upright",
    "rough2", "silky", "coarse", "fine", "ragged", "smooth3", "downy", "prickly",
    "syrupy", "watery", "creamy", "frothy", "foamy", "thick", "thin", "viscous",
    "stoic", "joyful", "weary", "cheery", "somber", "merry", "anxious", "serene",
    "patient", "restless", "vigilant", "drowsy", "alert", "groggy", "spry", "languid",
    "snowy", "rainy", "dewy", "dusty", "ashen", "smoky", "misty", "frosty",
    "crispy", "tender", "tough2", "chewy", "flaky", "crunchy", "soft", "firm",
];
