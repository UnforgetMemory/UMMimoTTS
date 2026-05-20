pub fn count_tokens_approx(text: &str) -> usize {
    // 简单估算：中文约 1.5 token/字，英文约 0.75 token/词
    let chinese_chars = text
        .chars()
        .filter(|c| *c >= '\u{4e00}' && *c <= '\u{9fff}')
        .count();
    let english_words = text.split_whitespace().count();

    (chinese_chars as f64 * 1.5) as usize + (english_words as f64 * 0.75) as usize
}

pub fn count_chars_detailed(text: &str) -> (usize, usize, usize, usize) {
    let total = text.chars().count();
    let chinese = text
        .chars()
        .filter(|c| *c >= '\u{4e00}' && *c <= '\u{9fff}')
        .count();
    let english = text.chars().filter(|c| c.is_ascii_alphabetic()).count();
    let other = total - chinese - english;

    (total, chinese, english, other)
}
