//! Wordlists for Mode 1 stego templates.
//!
//! Each list has exactly 256 entries (8 bits per slot fill). Words
//! are lowercase, single-token (no spaces, no punctuation), and
//! curated to read like normal Discord chat when slotted into the
//! Phase 2 chat-style templates. The earlier wordlists drew from a
//! "maximize entropy" pool (quetzal / vellum / regalia / kayak2)
//! which technically worked but visibly looked like a word-of-the-
//! day generator rather than chat.
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
    "apple", "banana", "lemon", "orange", "grape", "peach", "cherry",
    "berry", "melon", "mango", "pizza", "taco", "burger", "sandwich",
    "hotdog", "salad", "soup", "pasta", "sushi", "ramen", "bread",
    "toast", "bagel", "donut", "muffin", "cookie", "cake", "brownie",
    "pancake", "waffle", "omelet", "oatmeal", "cereal", "yogurt",
    "butter", "jam", "honey", "syrup", "chocolate", "popcorn",
    "candy", "pretzel", "cracker", "biscuit", "nacho", "fries",
    "salsa", "pickle", "olive", "coffee", "latte", "soda", "juice",
    "water", "milk", "smoothie", "lemonade", "slushie", "espresso",
    "tea", "dog", "cat", "bunny", "hamster", "parrot", "turtle",
    "lizard", "ferret", "kitten", "puppy", "squirrel", "raccoon",
    "robin", "sparrow", "duck", "swan", "penguin", "dolphin", "whale",
    "shark", "lobster", "crab", "octopus", "butterfly", "beetle",
    "dragonfly", "snail", "frog", "snake", "owl", "mom", "dad",
    "sister", "brother", "aunt", "uncle", "cousin", "grandma",
    "grandpa", "friend", "teacher", "coach", "boss", "manager",
    "coworker", "neighbor", "classmate", "roommate", "partner",
    "husband", "wife", "child", "kid", "baby", "toddler", "professor",
    "principal", "dentist", "doctor", "nurse", "apartment", "kitchen",
    "bedroom", "bathroom", "basement", "attic", "garage", "yard",
    "park", "cafe", "restaurant", "gym", "school", "library",
    "office", "hospital", "clinic", "airport", "beach", "lake",
    "hotel", "theater", "museum", "stadium", "arena", "church", "zoo",
    "mall", "market", "plaza", "phone", "laptop", "tablet", "charger",
    "headphones", "speaker", "remote", "keyboard", "monitor",
    "camera", "microphone", "controller", "console", "watch", "ring",
    "necklace", "bracelet", "glasses", "helmet", "jacket", "sweater",
    "hoodie", "jeans", "shorts", "dress", "shoes", "boots", "sneakers",
    "backpack", "wallet", "movie", "episode", "concert", "party",
    "game", "practice", "meeting", "exam", "project", "presentation",
    "interview", "vacation", "flight", "walk", "run", "nap", "errand",
    "chore", "laundry", "workout", "recital", "rehearsal", "festival",
    "audition", "wedding", "lunch", "dinner", "breakfast", "snack",
    "homework", "plan", "idea", "mood", "dream", "opinion", "secret",
    "joke", "prank", "surprise", "gift", "treat", "weekend", "monday",
    "tuesday", "friday", "saturday", "sunday", "summer", "winter",
    "holiday", "schedule", "deadline", "problem", "situation",
    "drama", "gossip", "story", "rumor", "memory", "vibe", "book",
    "magazine", "notebook", "pen", "marker", "scissors", "ruler",
    "lamp", "mirror", "window", "candle", "picture", "clock",
    "blender", "microwave", "fridge",
];

pub static ADJECTIVES: [&str; ADJ_COUNT] = [
    "tired", "happy", "sad", "excited", "nervous", "bored", "stressed",
    "calm", "anxious", "angry", "scared", "jealous", "lonely",
    "grateful", "hopeful", "proud", "ashamed", "embarrassed",
    "confused", "curious", "focused", "motivated", "lazy", "sleepy",
    "hungry", "thirsty", "sick", "exhausted", "energetic", "restless",
    "frustrated", "disappointed", "satisfied", "peaceful", "refreshed",
    "awake", "drowsy", "alert", "tense", "mellow", "smug", "defeated",
    "panicked", "hyped", "drained", "wired", "fried", "dazed", "giddy",
    "cranky", "grumpy", "chatty", "eager", "content", "dejected",
    "miserable", "ecstatic", "joyful", "gloomy", "somber", "good",
    "bad", "great", "terrible", "awesome", "awful", "nice", "fun",
    "boring", "cool", "lame", "weird", "normal", "crazy", "wild",
    "insane", "intense", "chill", "hectic", "easy", "tough", "simple",
    "tricky", "fancy", "basic", "premium", "cheap", "expensive",
    "smart", "dumb", "brilliant", "stupid", "silly", "serious",
    "classy", "trashy", "tacky", "elegant", "plain", "gorgeous",
    "pretty", "ugly", "beautiful", "hideous", "cute", "adorable",
    "handsome", "shiny", "dull", "sparkly", "bright", "dim", "dark",
    "colorful", "faded", "vivid", "vibrant", "muted", "neon", "pastel",
    "glossy", "matte", "blurry", "blinding", "glowing", "dazzling",
    "gleaming", "golden", "silver", "rusty", "painted", "polished",
    "scratched", "dented", "pristine", "smudged", "glistening",
    "twinkling", "shimmery", "sparkling", "big", "small", "tiny",
    "huge", "massive", "enormous", "giant", "large", "medium", "short",
    "tall", "long", "narrow", "wide", "thick", "thin", "skinny",
    "chubby", "lanky", "stocky", "slim", "hefty", "bulky", "slender",
    "petite", "jumbo", "smooth", "rough", "fluffy", "soft", "fuzzy",
    "silky", "prickly", "slimy", "sticky", "slippery", "dry", "moist",
    "wet", "oily", "greasy", "crispy", "crunchy", "chewy", "gooey",
    "mushy", "squishy", "bumpy", "lumpy", "woolly", "leathery",
    "rubbery", "hot", "cold", "warm", "icy", "frozen", "burning",
    "scalding", "boiling", "freezing", "chilly", "steamy", "lukewarm",
    "frosty", "sweet", "sour", "spicy", "savory", "bland", "fresh",
    "stale", "fragrant", "smelly", "stinky", "ripe", "rotten", "juicy",
    "tangy", "zesty", "smoky", "fast", "slow", "quick", "sluggish",
    "lively", "active", "still", "fierce", "mild", "gentle", "strong",
    "weak", "sturdy", "fragile", "wobbly", "shaky", "new", "old",
    "ancient", "modern", "classic", "retro", "vintage", "used", "worn",
    "broken", "fixed", "finished", "perfect", "flawed", "clean",
    "dirty", "neat", "messy", "organized",
];
