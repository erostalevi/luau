//! Extractive summary: a small TextRank over sentences.
//!
//! Deterministic, allocation-light and fast (microseconds for typical cards).

use std::collections::HashSet;

const MAX_SENTENCES: usize = 60;
const STOPWORDS: &[&str] = &[
    // en
    "the", "a", "an", "and", "or", "but", "of", "to", "in", "on", "for", "with", "is", "are", "was",
    "were", "be", "been", "it", "this", "that", "as", "at", "by", "from", "we", "you", "i", "they",
    "he", "she", "our", "your", "their", "not", "no", "so", "if", "then", "than", "do", "does",
    "can", "will", "should", "would", "could", "has", "have", "had", "there", "here", "what",
    // es
    "el", "la", "los", "las", "un", "una", "y", "o", "de", "del", "en", "con", "por", "para", "es",
    "son", "que", "se", "lo", "al", "su", "sus", "como", "pero", "más", "mas", "muy", "ya", "este",
    "esta", "esto", "hay", "sin", "sobre", "también", // pt
    "o", "os", "as", "um", "uma", "e", "do", "da", "dos", "das", "no", "na", "nos", "nas", "com",
    "em", "é", "são", "não", "mais", "muito", "isso", "esse", "essa",
];

fn words(s: &str, stop: &HashSet<&str>) -> HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() > 2)
        .map(str::to_lowercase)
        .filter(|w| !stop.contains(w.as_str()))
        .collect()
}

/// Split plain text into sentences on `.`, `!`, `?` followed by whitespace, and on newlines.
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        let chars: Vec<char> = para.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            cur.push(c);
            let end = matches!(c, '.' | '!' | '?' | '。')
                && chars.get(i + 1).is_none_or(|n| n.is_whitespace());
            if end {
                let t = cur.trim();
                if !t.is_empty() {
                    out.push(t.to_string());
                }
                cur.clear();
            }
        }
        let t = cur.trim();
        if !t.is_empty() {
            out.push(t.to_string());
        }
    }
    out
}

/// Pick up to `max` representative sentences, returned in document order,
/// truncated to `max_chars` total.
pub fn summarize(text: &str, max: usize, max_chars: usize) -> String {
    let mut sents = sentences(text);
    sents.retain(|s| s.chars().count() >= 3);
    sents.truncate(MAX_SENTENCES);
    if sents.is_empty() {
        return String::new();
    }
    if sents.len() <= max {
        return truncate(&sents.join(" "), max_chars);
    }
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    let bags: Vec<HashSet<String>> = sents.iter().map(|s| words(s, &stop)).collect();
    let n = sents.len();
    let mut weights = vec![vec![0.0f64; n]; n];
    for i in 0..n {
        for j in (i + 1)..n {
            let (a, b) = (&bags[i], &bags[j]);
            if a.is_empty() || b.is_empty() {
                continue;
            }
            let overlap = a.intersection(b).count() as f64;
            if overlap == 0.0 {
                continue;
            }
            let w = overlap / ((a.len() as f64).ln_1p() + (b.len() as f64).ln_1p());
            weights[i][j] = w;
            weights[j][i] = w;
        }
    }
    let out_sum: Vec<f64> = weights.iter().map(|r| r.iter().sum()).collect();
    let mut score = vec![1.0f64; n];
    for _ in 0..30 {
        let mut next = vec![0.15f64; n];
        for i in 0..n {
            let mut acc = 0.0;
            for j in 0..n {
                if weights[j][i] > 0.0 && out_sum[j] > 0.0 {
                    acc += weights[j][i] / out_sum[j] * score[j];
                }
            }
            next[i] += 0.85 * acc;
        }
        score = next;
    }
    // Slight lead bias: earlier sentences tend to be more informative.
    for (i, s) in score.iter_mut().enumerate() {
        *s *= 1.0 + 0.3 / (1.0 + i as f64);
    }
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| {
        score[b]
            .partial_cmp(&score[a])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    let mut pick: Vec<usize> = idx.into_iter().take(max).collect();
    pick.sort_unstable();
    let joined: Vec<&str> = pick.iter().map(|&i| sents[i].as_str()).collect();
    truncate(&joined.join(" "), max_chars)
}

pub fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    if let Some(p) = t.rfind(' ').filter(|&p| p > max_chars / 2) {
        t.truncate(p);
    }
    t.push('…');
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sentences() {
        let s = sentences("Hello world. This is v1.2 of it! Is it? yes\nNew line");
        assert_eq!(
            s,
            vec![
                "Hello world.",
                "This is v1.2 of it!",
                "Is it?",
                "yes",
                "New line"
            ]
        );
    }

    #[test]
    fn short_text_returned_whole() {
        assert_eq!(
            summarize("Just one sentence.", 2, 200),
            "Just one sentence."
        );
        assert_eq!(summarize("", 2, 200), "");
    }

    #[test]
    fn picks_central_sentences_in_order() {
        let text = "The login flow fails on Safari. Cats are nice. The login flow uses OAuth tokens. \
                    Tokens expire and the login flow breaks. Lunch was good.";
        let s = summarize(text, 2, 300);
        assert!(s.contains("login"), "{s}");
        assert!(!s.contains("Cats"), "{s}");
        assert!(!s.contains("Lunch"), "{s}");
    }

    #[test]
    fn truncates() {
        let t = truncate("alpha beta gamma delta epsilon", 12);
        assert!(t.chars().count() <= 12);
        assert!(t.ends_with('…'));
    }
}
