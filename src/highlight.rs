use std::{
    ops::{Bound, RangeBounds},
    slice::SliceIndex,
};
use thiserror::Error;
use tracing::error;

#[derive(Debug, PartialEq)]
pub enum Style<'a> {
    None(&'a str),
    Matched(&'a str),
}

#[derive(Error, Debug)]
pub enum MatchHighlightError {
    #[error("substring {start:?}:{end:?} not found in {string}")]
    SubstringNotFound {
        start: Bound<usize>,
        end: Bound<usize>,
        string: String,
    },
}

fn get_substring<R: RangeBounds<usize> + SliceIndex<str> + Clone>(
    string: &str,
    range: R,
) -> Result<&<R as SliceIndex<str>>::Output, MatchHighlightError> {
    string.get(range.clone()).ok_or_else(|| {
        let start = range.start_bound().cloned();
        let end = range.end_bound().cloned();
        error! {"Invalid range {:?}:{:?} in `{}`", start, end, string};
        MatchHighlightError::SubstringNotFound {
            start,
            end,
            string: string.into(),
        }
    })
}

pub fn sections_from_stringdices<'a>(
    string: &'a str,
    indices: &'a [usize],
) -> Result<Vec<Style<'a>>, MatchHighlightError> {
    let mut ret = Vec::new();
    let mut indices = indices.iter().peekable();
    let mut i: usize = 0;
    while let Some(m) = indices.next() {
        if *m > 0 {
            let sub = get_substring(string, i..*m)?;
            ret.push(Style::None(sub));
        }
        i = *m;
        let mut j = *m + 1;
        while let Some(m) = indices.peek() {
            if *m > &j {
                break;
            }
            indices.next();
            j += 1;
        }
        let sub = get_substring(string, i..j)?;
        ret.push(Style::Matched(sub));
        i = j;
    }
    if i < string.len() {
        let sub = get_substring(string, i..)?;
        ret.push(Style::None(sub));
    }
    Ok(ret)
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;

    #[test]
    fn no_highlight() -> Result<()> {
        assert_eq!(
            sections_from_stringdices("abc", &Vec::new())?,
            vec![Style::None("abc")]
        );
        Ok(())
    }

    #[test]
    fn highlight_one_char_at_start() -> Result<()> {
        assert_eq!(
            sections_from_stringdices("abc", &[0])?,
            vec![Style::Matched("a"), Style::None("bc")]
        );
        Ok(())
    }

    #[test]
    fn highlight_one_char_at_end() -> Result<()> {
        assert_eq!(
            sections_from_stringdices("abc", &[2])?,
            vec![Style::None("ab"), Style::Matched("c")]
        );
        Ok(())
    }

    #[test]
    fn highlight_three_char_at_start() -> Result<()> {
        assert_eq!(
            sections_from_stringdices("abcde", &[0, 1, 2])?,
            vec![Style::Matched("abc"), Style::None("de")]
        );
        Ok(())
    }

    #[test]
    fn highlight_three_char_at_end() -> Result<()> {
        assert_eq!(
            sections_from_stringdices("abcde", &[2, 3, 4])?,
            vec![Style::None("ab"), Style::Matched("cde")]
        );
        Ok(())
    }

    #[test]
    fn highlight_fun_mixture_one() -> Result<()> {
        assert_eq!(
            sections_from_stringdices("abcdefghijk", &[1, 2, 5, 6, 7, 9])?,
            vec![
                Style::None("a"),
                Style::Matched("bc"),
                Style::None("de"),
                Style::Matched("fgh"),
                Style::None("i"),
                Style::Matched("j"),
                Style::None("k")
            ]
        );
        Ok(())
    }

    // These 2 bugs occurred when starting a search with 's'
    // Note the first s in both cases is after a special character
    #[ignore]
    #[test]
    fn found_bug_1() -> Result<()> {
        // This used to error
        sections_from_stringdices("Chimay Grande Réserve", &[0, 16])?;
        Ok(())
    }

    // It looks like unicode is the cause?
    #[ignore]
    #[test]
    fn found_bug_2() -> Result<()> {
        // This used to error
        sections_from_stringdices("Bell’s Expedition", &[0, 5])?;
        Ok(())
    }

    #[test]
    fn periods_are_ok() -> Result<()> {
        sections_from_stringdices("ABC.DEF.GHI", &[0, 4])?;
        Ok(())
    }

    #[test]
    fn normal_apostrophes_are_ok() -> Result<()> {
        sections_from_stringdices("ABC'DEF.GHI", &[0, 4])?;
        Ok(())
    }

    #[test]
    fn normal_backticks_are_ok() -> Result<()> {
        sections_from_stringdices("ABC`DEF.GHI", &[0, 4])?;
        Ok(())
    }
}
