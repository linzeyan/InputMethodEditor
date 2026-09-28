//! Custom phrases: a code typed in letters, then Space, types its phrase.

use std::collections::HashMap;

/// In the user dir, a phrase a line: the code, a space, the phrase.
pub const PHRASES_FILE: &str = "custom_phrase.dat";

/// The phrases in `text` by code, or what is wrong with the first line that
/// isn't one. Blank lines and those starting with # are skipped.
pub fn parse(text: &str) -> Result<HashMap<String, String>, String> {
    let mut phrases = HashMap::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let wrong = |why: &str| format!("第 {} 行{why}：{line}", index + 1);
        let Some((code, phrase)) = line.split_once(char::is_whitespace) else {
            return Err(wrong("少了文字"));
        };
        if !code.chars().all(|it| it.is_ascii_alphabetic()) {
            return Err(wrong("的縮寫不是英文字母"));
        }
        // The letters typed are matched in lower case.
        let code = code.to_ascii_lowercase();
        if phrases
            .insert(code, phrase.trim_start().to_owned())
            .is_some()
        {
            return Err(wrong("的縮寫重複"));
        }
    }
    Ok(phrases)
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn codes_type_their_phrases() {
        let phrases = parse("# 地址\n\nAddr 臺北市 信義區\r\nsig\t— 簽名\n").unwrap();
        assert_eq!(phrases["addr"], "臺北市 信義區");
        assert_eq!(phrases["sig"], "— 簽名");
        assert_eq!(phrases.len(), 2);
    }

    #[test]
    fn lines_that_are_no_phrase_are_named() {
        assert_eq!(parse("ok 好\naddr").unwrap_err(), "第 2 行少了文字：addr");
        assert_eq!(
            parse("a1 地址").unwrap_err(),
            "第 1 行的縮寫不是英文字母：a1 地址"
        );
        // Only one phrase can be typed for a code.
        assert_eq!(parse("a 甲\nA 乙").unwrap_err(), "第 2 行的縮寫重複：A 乙");
    }
}
