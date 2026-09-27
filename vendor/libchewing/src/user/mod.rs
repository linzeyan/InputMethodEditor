//! User history and dictionary.

pub(crate) mod dict;
pub(crate) mod history_dict;
pub(crate) mod migrate;

pub use self::dict::{UserDict, UserDictError};
pub use self::history_dict::{HistoryDict, HistoryDictError};
pub use self::migrate::MigrateV4Error;
pub use self::migrate::migrate_v3_to_v4;
pub use self::migrate::should_migrate_from_v3;

use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Included, Unbounded};

use crate::zhuyin::{Syllable, SyllableVec};

/// Records whose syllables `query` can mean, one by one (see
/// [`Syllable::fuzzy_matches`]).
pub(crate) fn fuzzy_records<'a, V>(
    records: &'a BTreeMap<SyllableVec, V>,
    query: &'a [Syllable],
) -> impl Iterator<Item = (&'a SyllableVec, &'a V)> + 'a {
    // Syllables sort by initial first, so only keys whose first syllable
    // has the query's initial need a look. Those without one sort first.
    let initial = query
        .first()
        .map_or(0, |syl| syl.to_u16() & 0b0111111_00_0000_000);
    let bound = |value: u16| Syllable::try_from(value).map(|syl| [syl]);
    let (start, end) = (bound(initial), bound(initial + (1 << 9)));
    let start = start
        .as_ref()
        .map_or(Unbounded, |start| Included(&start[..]));
    let end = end.as_ref().map_or(Unbounded, |end| Excluded(&end[..]));
    records
        .range::<[Syllable], _>((start, end))
        .filter(move |(syllables, _)| {
            !query.is_empty()
                && syllables.len() == query.len()
                && syllables
                    .iter()
                    .zip(query)
                    .all(|(syl, q)| syl.fuzzy_matches(*q))
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::fuzzy_records;
    use crate::syl;
    use crate::zhuyin::{Bopomofo::*, Syllable, SyllableVec};

    fn key(syllables: &[Syllable]) -> SyllableVec {
        let mut key = SyllableVec::new();
        syllables.iter().for_each(|&syl| key.push(syl));
        key
    }

    fn matches(
        records: &BTreeMap<SyllableVec, &'static str>,
        query: &[Syllable],
    ) -> Vec<&'static str> {
        fuzzy_records(records, query)
            .map(|(_, &word)| word)
            .collect()
    }

    #[test]
    fn fuzzy_records_match_each_syllable_and_the_length() {
        let records = BTreeMap::from([
            (key(&[syl![I, TONE2]]), "移"),
            (key(&[syl![I, OU, TONE4]]), "又"),
            (key(&[syl![IU, TONE3]]), "雨"),
            (key(&[syl![I, TONE2], syl![D, U, TONE4]]), "移動"),
            (key(&[syl![SH, U]]), "書"),
        ]);
        // Syllables without an initial sort before the rest.
        assert_eq!(matches(&records, &[syl![I]]), ["移"]);
        assert_eq!(
            matches(&records, &[syl![I].abbreviate()]),
            ["移", "又", "雨"]
        );
        assert_eq!(
            matches(&records, &[syl![I], syl![D].abbreviate()]),
            ["移動"]
        );
        assert_eq!(matches(&records, &[syl![SH].abbreviate()]), ["書"]);
        assert!(matches(&records, &[]).is_empty());
    }
}
