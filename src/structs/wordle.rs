use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::Result;

#[derive(Debug, Clone, Copy)]
pub enum LetterStatus {
    None,
    Wrong,
    CorrectLetter,
    CorrectSpot,
}

impl ToSql for LetterStatus {
    #[inline]
    fn to_sql(&self) -> Result<ToSqlOutput<'_>> {
        let val: i64 = match *self {
            LetterStatus::None => 0,
            LetterStatus::Wrong => 1,
            LetterStatus::CorrectLetter => 2,
            LetterStatus::CorrectSpot => 3,
        };

        Ok(ToSqlOutput::from(val))
    }
}

impl FromSql for LetterStatus {
    #[inline]
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match i64::column_result(value)? {
            0 => Ok(LetterStatus::None),
            1 => Ok(LetterStatus::Wrong),
            2 => Ok(LetterStatus::CorrectLetter),
            3 => Ok(LetterStatus::CorrectSpot),
            i => Err(FromSqlError::OutOfRange(i)),
        }
    }
}

impl Default for LetterStatus {
    fn default() -> Self {
        LetterStatus::None
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WordleBoard(pub [[LetterStatus; 5]; 6]);

impl IntoIterator for WordleBoard {
    type Item = LetterStatus;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.concat().into_iter()
    }
}

#[derive(Debug, Default)]
pub struct Wordle {
    pub number: u32,
    pub score: u32,
    pub hardmode: bool,
    pub board: WordleBoard,
}
