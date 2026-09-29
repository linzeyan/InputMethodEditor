use std::sync::Arc;

use crate::{
    dictionary::LookupStrategy,
    lm::StaticDict,
    model::{Candidate, WordId},
    user::{HistoryDict, UserDict},
    zhuyin::Syllable,
};

#[derive(Clone, Debug)]
pub struct CompositeDict {
    inner: Arc<CompositeDictInner>,
}

#[derive(Debug)]
struct CompositeDictInner {
    static_dict: StaticDict,
    rare_dict: StaticDict,
    history_dict: HistoryDict,
    user_dict: UserDict,
    /// Characters the static dictionaries lack, read from Unihan: they come
    /// after every other word.
    unihan_dict: UserDict,
}

impl CompositeDict {
    pub fn new(
        static_dict: StaticDict,
        rare_dict: StaticDict,
        history_dict: HistoryDict,
        user_dict: UserDict,
        unihan_dict: UserDict,
    ) -> CompositeDict {
        CompositeDict {
            inner: Arc::new(CompositeDictInner {
                static_dict,
                rare_dict,
                history_dict,
                user_dict,
                unihan_dict,
            }),
        }
    }

    /// Words `syllables` can mean in a fuzzy lookup, each with the syllables
    /// the static, rare or user dictionary spells it with.
    pub(crate) fn fuzzy_readings(&self, syllables: &[Syllable]) -> Vec<(Vec<Syllable>, WordId)> {
        let strategy = LookupStrategy::FuzzyPartialPrefix;
        let mut readings = self.inner.static_dict.lookup_readings(syllables, strategy);
        readings.extend(self.inner.rare_dict.lookup_readings(syllables, strategy));
        readings.extend(self.inner.user_dict.fuzzy_readings(syllables));
        readings.extend(self.inner.unihan_dict.fuzzy_readings(syllables));
        readings
    }

    pub fn lookup(&self, syllables: &[Syllable], strategy: LookupStrategy) -> Vec<Candidate> {
        // base value
        let mut res: Vec<_> = self
            .inner
            .static_dict
            .lookup(syllables, strategy)
            .into_iter()
            .map(|wid| (wid, f64::NEG_INFINITY, None))
            .collect();
        // rare boost
        for wid in self.inner.rare_dict.lookup(syllables, strategy) {
            const RARE_BOOST: i8 = -100;
            if let Some(pos) = res.iter().position(|cand| cand.0 == wid) {
                res[pos].2 = Some(RARE_BOOST);
            } else {
                res.push((wid, f64::NEG_INFINITY, Some(RARE_BOOST)));
            }
        }
        // history boost
        for (wid, hist_prob) in self.inner.history_dict.unigram(syllables, strategy) {
            if let Some(pos) = res.iter().position(|cand| cand.0 == wid) {
                res[pos].1 = hist_prob;
            } else {
                res.push((wid, hist_prob, None));
            }
        }
        // user boost
        for (wid, user_pref) in self.inner.user_dict.lookup(syllables, strategy) {
            if let Some(pos) = res.iter().position(|w| w.0 == wid) {
                res[pos].2 = Some(user_pref);
            } else {
                res.push((wid, f64::NEG_INFINITY, Some(user_pref)));
            }
        }
        // Last, and only if not learned: a word the user typed keeps the place
        // that earned it.
        for (wid, boost) in self.inner.unihan_dict.lookup(syllables, strategy) {
            if !res.iter().any(|cand| cand.0 == wid) {
                res.push((wid, f64::NEG_INFINITY, Some(boost)));
            }
        }
        res.into_iter()
            .map(|cand| Candidate::Word {
                wid: cand.0,
                hist_prob: cand.1,
                user_pref: cand.2,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::CompositeDict;
    use crate::{
        dictionary::{LookupStrategy, StringTableBuilder},
        lm::{StaticDict, StaticDictBuilder},
        model::Candidate,
        syl,
        user::{HistoryDict, UserDict},
        zhuyin::Bopomofo::*,
    };

    #[test]
    fn unihan_words_come_last_until_the_user_learns_them() {
        let mut strings = StringTableBuilder::new();
        strings.insert("蹦");
        let strings = strings.build();
        let beng = syl![B, ENG, TONE4];
        let mut static_dict = StaticDictBuilder::new();
        static_dict.insert(&[beng], strings.get_wid("蹦").unwrap());
        let user_dict = UserDict::new(strings.clone());
        let unihan_dict =
            UserDict::from_reader("䨻,ㄅㄥˋ,-100\n".as_bytes(), strings.clone()).unwrap();
        let dict = CompositeDict::new(
            static_dict.build(),
            StaticDict::new(),
            HistoryDict::new(strings.clone()),
            user_dict.clone(),
            unihan_dict,
        );
        let words = || -> Vec<(String, Option<i8>)> {
            dict.lookup(&[beng], LookupStrategy::Standard)
                .into_iter()
                .map(|cand| match cand {
                    Candidate::Word { wid, user_pref, .. } => {
                        (strings.get_text(wid).unwrap(), user_pref)
                    }
                    _ => unreachable!(),
                })
                .collect()
        };
        assert_eq!(words(), [("蹦".into(), None), ("䨻".into(), Some(-100))]);
        // Once added as the user's word, it ranks like one.
        user_dict.insert(&[beng], "䨻");
        assert_eq!(words(), [("蹦".into(), None), ("䨻".into(), Some(10))]);
    }
}
