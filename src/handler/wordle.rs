use super::{log_error, Handler};
use crate::db::DB;
use crate::structs::wordle::{LetterStatus, Wordle, WordleBoard};
use lazy_static::lazy_static;
use regex::Regex;
use serenity::model::channel::Message;
use unicode_segmentation::UnicodeSegmentation;

#[inline(always)]
fn parse_square(text: &str) -> Result<LetterStatus, String> {
    match text {
        "⬛" | "⬜" => Ok(LetterStatus::Wrong),
        "🟨" | "🟦" => Ok(LetterStatus::CorrectLetter),
        "🟧" | "🟩" => Ok(LetterStatus::CorrectSpot),
        other => Err(format!("Invalid input for square parsing {}", other)),
    }
}
#[inline(always)]
fn is_unicode_newline(text: &str) -> bool {
    matches!(text, "\r\n" | "\n" | "\x0b" | "\r")
}

fn generate_board(text: &str) -> Result<WordleBoard, String> {
    let letters = UnicodeSegmentation::graphemes(text, true)
        .filter(|c| !is_unicode_newline(c))
        .map(parse_square)
        .collect::<Result<Vec<LetterStatus>, _>>()?;

    if letters.len() % 5 != 0 {
        Err(format!(
            "Invalid number of characters in row {}",
            letters.len()
        ))
    } else {
        let mut ret: [[LetterStatus; 5]; 6] = Default::default();
        for (i, letter) in letters.iter().enumerate() {
            let row = i / 5;
            let col = i % 5;
            ret[row][col] = *letter;
        }
        Ok(WordleBoard(ret))
    }
}

fn parse_wordle(text: &str) -> Result<Wordle, String> {
    lazy_static! {
        static ref RE: Regex = Regex::new(
            r"Wordle (\d{1,4}) ([123456X])/[123456](\*?)(\r\n|\n|\x0b|\f|\r|\x85){2}(([⬜⬛🟦🟨🟧🟩]+(\r\n|\n|\x0b|\f|\r|\x85)?){1,6})"
        )
        .unwrap();
    }

    let caps = RE.captures(text).ok_or("Text didn't match wordle")?;

    let number = caps
        .get(1)
        .ok_or("Couldn't unwrap number")?
        .as_str()
        .parse::<u32>()
        .map_err(|_| "Couldn't parse number");

    let score = match caps.get(2).ok_or("Couldn't unwrap score")?.as_str() {
        "X" => Ok(0),
        "1" => Ok(1),
        "2" => Ok(2),
        "3" => Ok(3),
        "4" => Ok(4),
        "5" => Ok(5),
        "6" => Ok(6),
        // In principle this should never be possible as the regex shouldn't allow this
        err => Err(format!("Invalid score parsed: {}", err)),
    };

    let hardmode_flag = caps.get(3).ok_or("Couldn't unwrap hardmode")?.as_str();

    let board = generate_board(caps.get(5).ok_or("Couldn't unwrap board")?.as_str());

    Ok(Wordle {
        number: number?,
        score: score?,
        board: board?,
        hardmode: hardmode_flag == "*",
    })
}

impl Handler {
    pub fn check_wordle(&self, msg: &Message) {
        if let Ok(w) = parse_wordle(&msg.content) {
            log_error(
                DB::db_call(|db| db.insert_wordle(*msg.id.as_u64(), &w)),
                "Insert wordle",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_wordle_basic() -> Result<(), String> {
        let wordle_str = "Wordle 211 3/6\n\n🟩🟩🟨🟨⬜\n🟩🟩🟨🟩🟨\n🟩🟩🟩🟩🟩";
        let wordle = parse_wordle(wordle_str)?;

        assert_eq!(wordle.number, 211);
        assert_eq!(wordle.score, 3);
        assert!(!wordle.hardmode);
        Ok(())
    }
    #[test]
    fn test_wordle_colour_blind() -> Result<(), String> {
        let wordle_str = "Wordle 212 4/6\n\n⬛🟦⬛⬛🟦\n⬛🟦🟧⬛🟧\n🟧⬛🟧⬛🟧\n🟧🟧🟧🟧🟧";
        let wordle = parse_wordle(wordle_str)?;

        assert_eq!(wordle.number, 212);
        assert_eq!(wordle.score, 4);
        assert!(!wordle.hardmode);
        Ok(())
    }
    #[test]
    fn test_wordle_dark_mode() -> Result<(), String> {
        let wordle_str = "Wordle 212 3/6\n\n🟩⬛⬛🟩🟩\n🟩⬛🟩🟩🟩\n🟩🟩🟩🟩🟩";
        let wordle = parse_wordle(wordle_str)?;

        assert_eq!(wordle.number, 212);
        assert_eq!(wordle.score, 3);
        assert!(!wordle.hardmode);
        Ok(())
    }

    #[test]
    fn test_wordle_hard_mode() -> Result<(), String> {
        let wordle_str = "Wordle 217 3/6*\n\n⬜🟩⬜⬜⬜\n⬜🟩🟩🟨⬜\n🟩🟩🟩🟩🟩";
        let wordle = parse_wordle(wordle_str)?;

        assert_eq!(wordle.number, 217);
        assert_eq!(wordle.score, 3);
        assert!(wordle.hardmode);
        Ok(())
    }
}
