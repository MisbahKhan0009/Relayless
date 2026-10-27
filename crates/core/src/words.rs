//! Safety words: a short phrase both devices derive from their two public
//! keys. If the words match on both screens, nobody is sitting in the middle
//! of the connection. Five words from a 256-word list give 40 bits.

const WORDS: [&str; 256] = [
    "acorn", "amber", "anchor", "apple", "arrow", "aspen", "atlas", "autumn",
    "badge", "bamboo", "banjo", "barley", "basil", "beacon", "bean", "bear",
    "beetle", "berry", "birch", "bison", "blossom", "bolt", "bonsai", "bottle",
    "breeze", "brick", "bridge", "brook", "bubble", "buckle", "cabin", "cactus",
    "camel", "candle", "canoe", "canyon", "carbon", "cargo", "carrot", "castle",
    "cedar", "cello", "chalk", "cherry", "chess", "cider", "cinder", "citrus",
    "clay", "cliff", "clover", "cobalt", "cocoa", "comet", "compass", "copper",
    "coral", "cotton", "cougar", "crane", "crayon", "cricket", "crystal", "cypress",
    "daisy", "delta", "denim", "desert", "dolphin", "domino", "donkey", "dragon",
    "drum", "dune", "eagle", "easel", "echo", "eclipse", "elbow", "elder",
    "ember", "emerald", "falcon", "fern", "ferry", "fiddle", "fig", "finch",
    "fjord", "flame", "flute", "forest", "fossil", "fox", "galaxy", "garden",
    "garlic", "gecko", "geyser", "ginger", "glacier", "globe", "goose", "granite",
    "grape", "gravel", "harbor", "harp", "hazel", "heron", "hill", "honey",
    "hornet", "husky", "iceberg", "igloo", "indigo", "iris", "island", "ivory",
    "jade", "jaguar", "jasmine", "jelly", "jungle", "kayak", "kelp", "kettle",
    "kiwi", "koala", "ladder", "lagoon", "lantern", "lava", "lemon", "lentil",
    "lily", "lime", "linen", "lizard", "llama", "lotus", "lynx", "magnet",
    "mango", "maple", "marble", "meadow", "melon", "meteor", "mint", "mitten",
    "moose", "moss", "muffin", "nectar", "needle", "nickel", "noodle", "nutmeg",
    "oak", "oasis", "ocean", "olive", "onyx", "orbit", "orchid", "otter",
    "owl", "oyster", "paddle", "panda", "papaya", "parrot", "peach", "pebble",
    "pecan", "pelican", "pepper", "piano", "pickle", "pigeon", "pillow", "pine",
    "planet", "plum", "pony", "poppy", "prairie", "prism", "puffin", "pumpkin",
    "quail", "quartz", "quill", "rabbit", "radish", "raft", "rain", "raven",
    "reef", "rhino", "ribbon", "river", "robin", "rocket", "rose", "ruby",
    "saddle", "saffron", "salmon", "sand", "sapphire", "satin", "scarf", "seal",
    "shell", "silver", "sketch", "sled", "snow", "sparrow", "spruce", "squid",
    "star", "stone", "storm", "summit", "sunset", "swan", "tango", "teapot",
    "thistle", "thunder", "tiger", "timber", "toast", "tomato", "topaz", "tulip",
    "tundra", "turtle", "valley", "velvet", "violet", "volcano", "waffle", "walnut",
    "walrus", "willow", "window", "winter", "wolf", "yogurt", "zebra", "zinnia",
];

pub const COUNT: usize = 5;

/// Same result on both devices: the keys are sorted before hashing.
pub fn safety_words(a: &[u8; 32], b: &[u8; 32]) -> Vec<String> {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut h = blake3::Hasher::new();
    h.update(b"relayless-safety-words-v1");
    h.update(lo);
    h.update(hi);
    let d = h.finalize();
    d.as_bytes()[..COUNT].iter().map(|&i| WORDS[i as usize].to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symmetric_and_distinct() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let c = [3u8; 32];
        assert_eq!(safety_words(&a, &b), safety_words(&b, &a));
        assert_ne!(safety_words(&a, &b), safety_words(&a, &c));
        assert_eq!(safety_words(&a, &b).len(), COUNT);
        let mut all: Vec<&str> = WORDS.to_vec();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 256);
    }
}
