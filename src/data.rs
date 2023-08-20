use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use indexmap::IndexMap;
use rayon::prelude::*;
use std::{borrow::Cow, cmp::Ordering};
use tui::widgets::ListState;

/// Type for holding fuzzy match score with corresponding indices
pub struct FuzzyScore {
    /// fuzzy match score
    pub score: i64,
    /// fuzzy match indices (positions in the matched string)
    pub indices: Vec<usize>,
}

impl Ord for FuzzyScore {
    fn cmp(&self, other: &Self) -> Ordering {
        // reverse so ascending order is highest score first!!!
        other.score.cmp(&self.score)
    }
}

impl PartialOrd for FuzzyScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for FuzzyScore {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
    }
}

impl Eq for FuzzyScore {}

/// Return type for `FuzzyFinder::selection`
#[derive(Clone)]
pub struct FuzzyListEntry<'a> {
    /// value of entry
    pub value: &'a str, // TODO not a &str?
    /// fuzzy match score
    pub score: i64,
    /// fuzzy match indices (positions in `value`)
    pub indices: Vec<usize>,
}

/// State for `FuzzyList<K>`.  Hold on to one of these and pass to `render_stateful_widget`
///
/// # Example
///
/// ```
/// use tui::prelude::*;
/// use tui::widgets::*;
/// use tuiscope::{FuzzyFinder, FuzzyList};
///
/// fn ui<B: Backend>(f: &mut Frame<B>, state: &mut FuzzyFinder) {
///     let chunks = Layout::default()
///         .direction(Direction::Vertical)
///         .constraints([Constraint::Min(1)].as_ref())
///         .split(f.size());
///
///     let fuzzy_results = FuzzyList::default()
///         .block(Block::default().borders(Borders::ALL).title("Options"))
///         .matched_char_style(Style::default().fg(Color::Cyan))
///         .selection_highlight_style(Style::default().add_modifier(Modifier::BOLD));
///     f.render_stateful_widget(fuzzy_results, chunks[2], state);
/// }
/// ```
#[derive(Default)]
pub struct FuzzyFinder<'a> {
    /// The current filter string.
    filter: Cow<'a, str>,
    /// IndexMap of FuzzyScore.
    pub matches: IndexMap<Cow<'a, str>, Option<FuzzyScore>>,
    /// State for the `FuzzyList` widget's selection.
    pub state: ListState,
}

impl<'a> FuzzyFinder<'a> {
    /// Clears the filter term.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.set_filter("foo");
    /// ff.clear_filter();
    /// ```
    pub fn clear_filter(&mut self) -> &mut Self {
        self.filter = Cow::default();
        self.update_matches(true);
        self
    }

    /// Resets the selected line from filtered options to the 0th.
    fn reset_selection(&mut self) -> &mut Self {
        if self.matches.is_empty() {
            self.state.select(None);
        } else {
            self.state.select(Some(0));
        }
        self
    }

    /// Select the next filtered entry.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.select_next();
    /// ```
    pub fn select_next(&mut self) -> &mut Self {
        if let Some(current) = self.state.selected() {
            self.select(current + 1);
        } else {
            self.reset_selection();
        }
        self
    }

    /// Select the previous filtered entry.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.select_next();
    /// ff.select_next();
    /// ff.select_prev();
    /// ```
    pub fn select_prev(&mut self) -> &mut Self {
        if let Some(current) = self.state.selected() {
            if current > 0 {
                self.select(current - 1);
            }
        } else {
            self.reset_selection();
        }
        self
    }

    fn select(&mut self, index: usize) -> &mut Self {
        let len = self.matches.len();
        if len < 1 {
            return self.reset_selection();
        }
        self.state.select(Some(std::cmp::min(index, len - 1)));
        self
    }

    /// Get the current selected entry.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.select_next();
    /// ff.select_next();
    /// ff.select_prev();
    /// let answer = ff.selection();
    /// ```
    pub fn selection(&self) -> Option<FuzzyListEntry> {
        self.state.selected().and_then(|i| {
            self.matches.get_index(i).and_then(|(value, score)| {
                score
                    .as_ref()
                    .map(|FuzzyScore { score, indices }| FuzzyListEntry {
                        value,
                        indices: indices.clone(),
                        score: *score,
                    })
            })
        })
    }

    /// Updates the filter term.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.set_filter("foo");
    /// ```
    pub fn set_filter<T: Into<Cow<'a, str>>>(&mut self, filter: T) -> &mut Self {
        self.filter = filter.into();
        self.update_matches(true);
        self
    }

    /// Updates the set of options to search by adding from an iterator.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.push_options(["abc", "bcd", "cde"]);
    /// ```
    pub fn push_options<T: 'a + IntoIterator<Item = R>, R: Into<Cow<'a, str>>>(
        &mut self,
        options: T,
    ) -> &mut Self {
        for option in options {
            self._push_option(option);
        }
        self.update_matches(false);
        self
    }

    /// Builder method which sets search options.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let ff = FuzzyFinder::default().with_options(["one", "two", "three"]);
    /// ```
    pub fn with_options<T: 'a + IntoIterator<Item = R>, R: Into<Cow<'a, str>>>(
        mut self,
        options: T,
    ) -> Self {
        self.set_options(options);
        self
    }

    /// Sets search options.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.set_options(["one", "two", "three"]);
    /// ```
    pub fn set_options<T: 'a + IntoIterator<Item = R>, R: Into<Cow<'a, str>>>(
        &mut self,
        options: T,
    ) -> &mut Self {
        // TODO be more efficient, keep any existing scores for overlapping keys.
        // Maybe leverage `remove_options` when an efficient  version of that has
        // been made.
        self.matches.clear();
        self.push_options(options);
        self
    }

    /// Add an option to search.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.push_option("hello");
    /// ```
    pub fn push_option<R: Into<Cow<'a, str>>>(&mut self, option: R) {
        self._push_option(option);
        self.update_matches(false);
    }

    /// Adds an option to search without updating.
    fn _push_option<R: Into<Cow<'a, str>>>(&mut self, option: R) {
        // keep existing score if entry exists.
        self.matches.entry(option.into()).or_insert(None);
    }

    /// Removes an option.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.push_options(["hello", "friend"]);
    /// ff.remove_option("hello");
    /// ```
    pub fn remove_option<R: AsRef<str>>(&mut self, key: R) {
        self.matches.shift_remove(key.as_ref());
    }

    /// Removes multiple options.
    ///
    /// # Example
    ///
    /// ```
    /// use tuiscope::FuzzyFinder;
    ///
    /// let mut ff = FuzzyFinder::default();
    /// ff.push_options(["hello", "my", "old", "friend"]);
    /// ff.remove_options(["my", "old"]);
    /// ```
    pub fn remove_options<T: 'a + IntoIterator<Item = R>, R: AsRef<str>>(&mut self, keys: T) {
        // TODO something smarter, this will O(n) shift all entries in `self.matches`
        // for each key.  Will in certain cases be better to just `self.matches.remove`
        // followed by a sort.
        for key in keys {
            self.remove_option(key);
        }
    }

    /// Computes new scores for all options if `new_filter_term` is true.
    /// Otherwise competes scores for all options who haven't had a calculation
    /// yet against the current filter.
    fn update_matches(&mut self, new_filter_term: bool) {
        let matcher = SkimMatcherV2::default();

        // TODO None matches were inserted last, so we should be able to iterate
        // from the end and stop early.  But I couldn't quite find the right
        // early-stopping option for an IndexedParallesIterator
        // iter = iter.rev().take_any_while... race behavior is not ideal
        self.matches
            .par_iter_mut()
            .filter(|(_, score)| new_filter_term || score.is_none())
            .for_each(|(value, score)| {
                *score = matcher
                    .fuzzy_indices(value, &self.filter)
                    .map(|(score, indices)| FuzzyScore { score, indices });
            });

        self.matches.par_sort_unstable_by(|_, v1, _, v2| match v1 {
            Some(v1) => match v2 {
                Some(v2) => v1.cmp(v2),
                None => Ordering::Less,
            },
            None => Ordering::Greater,
        });

        // TODO only if some change
        self.reset_selection();
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn remove_option() {
        let mut ff = FuzzyFinder::default();
        ff.push_options(["hello", "friend"]);
        assert!(ff.matches.contains_key("hello"));
        ff.remove_option("hello");
        assert!(!ff.matches.contains_key("hello"));
    }

    #[test]
    fn remove_options() {
        let mut ff = FuzzyFinder::default();
        ff.push_options(["hello", "my", "old", "friend"]);
        ff.remove_options(["my", "old"]);
        assert!(ff.matches.contains_key("hello"));
        assert!(ff.matches.contains_key("friend"));
        assert!(!ff.matches.contains_key("my"));
        assert!(!ff.matches.contains_key("old"));
    }
}
