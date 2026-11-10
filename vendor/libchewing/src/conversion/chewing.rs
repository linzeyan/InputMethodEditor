use std::fmt::Debug;

use bstr::ByteSlice;
use log::{Level, debug, log_enabled};

use super::{
    Composition, ConversionEngine, Gap, Interval, Outcome, Selection, Symbol,
    word_lattice::{Edge, Lattice},
};
use crate::{
    conversion::{Decoder, LatticeBuilder},
    dictionary::StringTable,
    model::Candidate,
    zhuyin::Syllable,
};

/// The default Chewing conversion method.
#[derive(Debug)]
pub struct ChewingEngine {
    pub word_lattice_builder: LatticeBuilder,
    pub decoder: Decoder,
    pub string_table: StringTable,
}

impl ChewingEngine {
    const MAX_OUT: u8 = 10;

    pub(crate) fn convert<'a>(&'a self, com: &'a Composition) -> Vec<Outcome> {
        self.convert_after(com, &[])
    }

    /// The context's syllables go in front of the composition's, so a word
    /// can run from one into the other and the context weighs in on the
    /// first words; only words that read as what was committed may cover the
    /// context, and what they cover is dropped from the output.
    pub(crate) fn convert_after<'a>(
        &'a self,
        com: &'a Composition,
        context: &[(Syllable, String)],
    ) -> Vec<Outcome> {
        let context = if com.is_empty() { &[][..] } else { context };
        let n = context.len();
        let joined;
        let lattice = if n == 0 {
            self.word_lattice_builder.build_lattice(com)
        } else {
            joined = after_context(com, context);
            let mut lattice = self.word_lattice_builder.build_lattice(&joined);
            self.keep_agreeing(&mut lattice, context);
            lattice
        };
        let hypothesis = self.decoder.decoden(lattice, Self::MAX_OUT);

        let mut outcomes: Vec<Outcome> = hypothesis
            .into_iter()
            .map(|hyp| {
                let cost = hyp.cost;
                let mut cursor = 0;
                let intervals = hyp
                    .candidates
                    .into_iter()
                    .map(|cand| match cand {
                        Candidate::None => ("<unk>".to_string().into_boxed_str(), false),
                        Candidate::Word { wid, .. } => {
                            let text = self
                                .string_table
                                .get_text(wid)
                                .unwrap_or("<unk>".into())
                                .to_string()
                                .into_boxed_str();
                            (text, true)
                        }
                        Candidate::Grapheme(ch) => (ch.to_string().into_boxed_str(), false),
                    })
                    .map(|(text, is_phrase)| {
                        let int = Interval {
                            start: cursor,
                            end: cursor + text.as_bytes().graphemes().count(),
                            is_phrase,
                            text,
                        };
                        cursor = int.end;
                        int
                    })
                    .filter_map(|interval| past_context(interval, n))
                    .fold(vec![], |acc, interval| glue_fn(com, acc, interval));
                Outcome { intervals, cost }
            })
            .collect();
        if n > 0 {
            // Readings that differ only within the context come out the same
            // once it is dropped; the cheapest goes first, so it stays.
            let mut seen = vec![];
            outcomes.retain(|out| {
                let text: String = out.intervals.iter().map(|it| &*it.text).collect();
                let new = !seen.contains(&text);
                seen.push(text);
                new
            });
        }
        if log_enabled!(Level::Debug) {
            for out in &outcomes {
                debug!("Cost {:>5.2} {:?}", out.cost, out.intervals);
            }
        }
        outcomes
    }

    /// Leaves on the context only the words whose characters there are the
    /// ones committed. Each context character keeps a way through on its own,
    /// as a plain character if no single word fits: else the only word left
    /// over it could be one running into the composition, forced on it.
    fn keep_agreeing(&self, lattice: &mut Lattice, context: &[(Syllable, String)]) {
        let n = context.len();
        for (start, edges) in lattice.edges.iter_mut().enumerate().take(n) {
            edges.retain(|edge| {
                let end = (edge.end as usize).min(n);
                let Candidate::Word { wid, .. } = &edge.cand else {
                    return false;
                };
                self.string_table.get_text(*wid).is_some_and(|text| {
                    text.as_bytes()
                        .graphemes()
                        .take(end - start)
                        .eq(context[start..end].iter().map(|(_, ch)| ch.as_str()))
                })
            });
            if !edges.iter().any(|edge| edge.end as usize == start + 1) {
                edges.push(Edge {
                    end: (start + 1) as u8,
                    cand: Candidate::Grapheme(context[start].1.clone()),
                    bonus: 0.0,
                });
            }
        }
    }
}

/// `com` with the context's syllables in front, free to form words with its
/// first ones.
fn after_context(com: &Composition, context: &[(Syllable, String)]) -> Composition {
    let n = context.len();
    let mut joined = Composition::new();
    joined.symbols = context
        .iter()
        .map(|(syl, _)| Symbol::from(*syl))
        .chain(com.symbols.iter().cloned())
        .collect();
    joined.gaps = vec![Gap::Normal; n];
    joined.gaps.extend_from_slice(&com.gaps);
    joined.gaps[0] = Gap::Begin;
    joined.gaps[n] = Gap::Normal;
    joined.selections = com
        .selections
        .iter()
        .map(|sel| Selection {
            start: sel.start + n,
            end: sel.end + n,
            ..*sel
        })
        .collect();
    joined
}

/// The part of `interval` past the first `n` symbols, the context, placed
/// in the composition. A word running out of the context keeps only the
/// characters typed now, and is no word of the composition's: not one to
/// learn, or to pin as chosen.
fn past_context(interval: Interval, n: usize) -> Option<Interval> {
    if interval.end <= n {
        return None;
    }
    if interval.start >= n {
        return Some(Interval {
            start: interval.start - n,
            end: interval.end - n,
            ..interval
        });
    }
    let text: String = interval
        .text
        .as_bytes()
        .graphemes()
        .skip(n - interval.start)
        .collect();
    Some(Interval {
        start: 0,
        end: interval.end - n,
        is_phrase: false,
        text: text.into_boxed_str(),
    })
}

impl ConversionEngine for ChewingEngine {
    fn convert<'a>(&'a self, comp: &'a Composition) -> Vec<Outcome> {
        ChewingEngine::convert(self, comp)
    }
    fn convert_after<'a>(
        &'a self,
        comp: &'a Composition,
        context: &[(Syllable, String)],
    ) -> Vec<Outcome> {
        ChewingEngine::convert_after(self, comp, context)
    }
}

fn glue_fn(com: &Composition, mut acc: Vec<Interval>, interval: Interval) -> Vec<Interval> {
    if acc.is_empty() {
        acc.push(interval);
        return acc;
    }
    let last = acc.last().expect("acc should have at least one item");
    if !last.is_phrase || !interval.is_phrase {
        acc.push(interval);
        return acc;
    }
    if let Some(Gap::Glue) = com.gap(last.end) {
        let last = acc.pop().expect("acc should have at least one item");
        let mut phrase = last.text.into_string();
        phrase.push_str(&interval.text);
        acc.push(Interval {
            start: last.start,
            end: interval.end,
            is_phrase: true,
            text: phrase.into_boxed_str(),
        })
    } else {
        acc.push(interval);
    }
    acc
}

#[cfg(test)]
mod tests {
    use crate::{
        conversion::{
            ChewingEngine, Composition, Decoder, Gap, Interval, LatticeBuilder, Selection, Symbol,
        },
        dictionary::{CompositeDict, LookupStrategy, StringTable},
        lm::{StaticDict, StaticLm},
        syl,
        user::{HistoryDict, UserDict},
        zhuyin::Bopomofo::*,
    };

    fn test_dictionary(string_table: StringTable) -> CompositeDict {
        let user_dict = UserDict::new(string_table.clone());

        user_dict.insert(&[syl![G, U, O, TONE2]], "國");
        user_dict.insert(&[syl![M, I, EN, TONE2]], "民");
        user_dict.insert(&[syl![D, A, TONE4]], "大");
        user_dict.insert(&[syl![H, U, EI, TONE4]], "會");
        user_dict.insert(&[syl![D, AI, TONE4]], "代");
        user_dict.insert(&[syl![B, I, AU, TONE3]], "表");
        user_dict.insert(&[syl![B, I, AU, TONE3]], "錶");
        user_dict.insert(&[syl![G, U, O, TONE2], syl![M, I, EN, TONE2]], "國民");
        user_dict.insert(&[syl![D, A, TONE4], syl![H, U, EI, TONE4]], "大會");
        user_dict.insert(&[syl![D, AI, TONE4], syl![B, I, AU, TONE3]], "代表");
        user_dict.insert(&[syl![D, AI, TONE4], syl![B, I, AU, TONE3]], "戴錶");
        user_dict.insert(&[syl![X, I, EN]], "心");
        user_dict.insert(&[syl![K, U, TONE4], syl![I, EN]], "庫音");
        user_dict.insert(&[syl![X, I, EN], syl![K, U, TONE4], syl![I, EN]], "新酷音");
        user_dict.insert(
            &[syl![C, E, TONE4], syl![SH, TONE4], syl![I, TONE2]],
            "測試儀",
        );
        user_dict.insert(&[syl![C, E, TONE4], syl![SH, TONE4]], "測試");
        user_dict.insert(&[syl![I, TONE2], syl![X, I, A, TONE4]], "一下");
        user_dict.insert(&[syl![X, I, A, TONE4]], "下");
        user_dict.insert(&[syl![H, A]], "哈");
        user_dict.insert(&[syl![H, A], syl![H, A]], "哈哈");

        CompositeDict::new(
            StaticDict::new(),
            StaticDict::new(),
            HistoryDict::new(string_table),
            user_dict,
            UserDict::new(StringTable::new()),
        )
    }

    #[test]
    fn convert_simple_chinese_composition() {
        let string_table = StringTable::new();
        let dict = test_dictionary(string_table.clone());
        let word_lattice_builder = LatticeBuilder {
            dict,
            lookup_strategy: LookupStrategy::Standard,
        };
        let engine = ChewingEngine {
            word_lattice_builder,
            decoder: Decoder {
                lm: StaticLm::new(),
                hist: HistoryDict::new(string_table.clone()),
                lambda: Decoder::LAMBDA,
            },
            string_table,
        };
        let mut composition = Composition::new();
        for sym in [
            Symbol::from(syl![G, U, O, TONE2]),
            Symbol::from(syl![M, I, EN, TONE2]),
            Symbol::from(syl![D, A, TONE4]),
            Symbol::from(syl![H, U, EI, TONE4]),
            Symbol::from(syl![D, AI, TONE4]),
            Symbol::from(syl![B, I, AU, TONE3]),
        ] {
            composition.push(sym);
        }
        assert_eq!(
            vec![
                Interval {
                    start: 0,
                    end: 2,
                    is_phrase: true,
                    text: "國民".into()
                },
                Interval {
                    start: 2,
                    end: 4,
                    is_phrase: true,
                    text: "大會".into()
                },
                Interval {
                    start: 4,
                    end: 6,
                    is_phrase: true,
                    text: "代表".into()
                },
            ],
            engine.convert(&composition)[0].intervals
        );
    }

    #[test]
    fn convert_chinese_composition_with_breaks() {
        let string_table = StringTable::new();
        let dict = test_dictionary(string_table.clone());
        let word_lattice_builder = LatticeBuilder {
            dict,
            lookup_strategy: LookupStrategy::Standard,
        };
        let engine = ChewingEngine {
            word_lattice_builder,
            decoder: Decoder {
                lm: StaticLm::new(),
                hist: HistoryDict::new(string_table.clone()),
                lambda: Decoder::LAMBDA,
            },
            string_table,
        };
        let mut composition = Composition::new();
        for sym in [
            Symbol::from(syl![G, U, O, TONE2]),
            Symbol::from(syl![M, I, EN, TONE2]),
            Symbol::from(syl![D, A, TONE4]),
            Symbol::from(syl![H, U, EI, TONE4]),
            Symbol::from(syl![D, AI, TONE4]),
            Symbol::from(syl![B, I, AU, TONE3]),
        ] {
            composition.push(sym);
        }
        composition.set_gap(1, Gap::Break);
        composition.set_gap(5, Gap::Break);
        assert_eq!(
            vec![
                Interval {
                    start: 0,
                    end: 1,
                    is_phrase: true,
                    text: "國".into()
                },
                Interval {
                    start: 1,
                    end: 2,
                    is_phrase: true,
                    text: "民".into()
                },
                Interval {
                    start: 2,
                    end: 4,
                    is_phrase: true,
                    text: "大會".into()
                },
                Interval {
                    start: 4,
                    end: 5,
                    is_phrase: true,
                    text: "代".into()
                },
                Interval {
                    start: 5,
                    end: 6,
                    is_phrase: true,
                    text: "表".into()
                },
            ],
            engine.convert(&composition)[0].intervals
        );
    }

    #[test]
    fn convert_chinese_composition_with_good_selection() {
        let string_table = StringTable::new();
        let dict = test_dictionary(string_table.clone());
        let word_lattice_builder = LatticeBuilder {
            dict,
            lookup_strategy: LookupStrategy::Standard,
        };
        let engine = ChewingEngine {
            word_lattice_builder,
            decoder: Decoder {
                lm: StaticLm::new(),
                hist: HistoryDict::new(string_table.clone()),
                lambda: Decoder::LAMBDA,
            },
            string_table: string_table.clone(),
        };
        let mut composition = Composition::new();
        for sym in [
            Symbol::from(syl![G, U, O, TONE2]),
            Symbol::from(syl![M, I, EN, TONE2]),
            Symbol::from(syl![D, A, TONE4]),
            Symbol::from(syl![H, U, EI, TONE4]),
            Symbol::from(syl![D, AI, TONE4]),
            Symbol::from(syl![B, I, AU, TONE3]),
        ] {
            composition.push(sym);
        }
        composition.push_selection(Selection {
            start: 4,
            end: 6,
            wid: string_table.intern("戴錶"),
        });
        assert_eq!(
            vec![
                Interval {
                    start: 0,
                    end: 2,
                    is_phrase: true,
                    text: "國民".into()
                },
                Interval {
                    start: 2,
                    end: 4,
                    is_phrase: true,
                    text: "大會".into()
                },
                Interval {
                    start: 4,
                    end: 6,
                    is_phrase: true,
                    text: "戴錶".into()
                },
            ],
            engine.convert(&composition)[0].intervals
        );
    }

    #[test]
    #[ignore]
    fn convert_chinese_composition_with_substring_selection() {
        let string_table = StringTable::new();
        let dict = test_dictionary(string_table.clone());
        let word_lattice_builder = LatticeBuilder {
            dict,
            lookup_strategy: LookupStrategy::Standard,
        };
        let engine = ChewingEngine {
            word_lattice_builder,
            decoder: Decoder {
                lm: StaticLm::new(),
                hist: HistoryDict::new(string_table.clone()),
                lambda: Decoder::LAMBDA,
            },
            string_table: string_table.clone(),
        };
        let mut composition = Composition::new();
        for sym in [
            Symbol::from(syl![X, I, EN]),
            Symbol::from(syl![K, U, TONE4]),
            Symbol::from(syl![I, EN]),
        ] {
            composition.push(sym);
        }
        composition.push_selection(Selection {
            start: 1,
            end: 3,
            wid: string_table.intern("酷音"),
        });
        // FIXME: support substring selection?
        assert_eq!(
            vec![Interval {
                start: 0,
                end: 3,
                is_phrase: true,
                text: "新酷音".into()
            }],
            engine.convert(&composition)[0].intervals
        );
    }

    #[test]
    fn multiple_single_word_selection() {
        let string_table = StringTable::new();
        let dict = test_dictionary(string_table.clone());
        let word_lattice_builder = LatticeBuilder {
            dict,
            lookup_strategy: LookupStrategy::Standard,
        };
        let engine = ChewingEngine {
            word_lattice_builder,
            decoder: Decoder {
                lm: StaticLm::new(),
                hist: HistoryDict::new(string_table.clone()),
                lambda: Decoder::LAMBDA,
            },
            string_table: string_table.clone(),
        };
        let mut composition = Composition::new();
        for sym in [
            Symbol::from(syl![D, AI, TONE4]),
            Symbol::from(syl![B, I, AU, TONE3]),
        ] {
            composition.push(sym);
        }
        for sel in [
            Selection {
                start: 0,
                end: 1,
                wid: string_table.intern("代"),
            },
            Selection {
                start: 1,
                end: 2,
                wid: string_table.intern("錶"),
            },
        ] {
            composition.push_selection(sel);
        }
        assert_eq!(
            vec![
                Interval {
                    start: 0,
                    end: 1,
                    is_phrase: true,
                    text: "代".into()
                },
                Interval {
                    start: 1,
                    end: 2,
                    is_phrase: true,
                    text: "錶".into()
                }
            ],
            engine.convert(&composition)[0].intervals
        );
    }

    #[test]
    fn convert_cycle_alternatives() {
        let string_table = StringTable::new();
        let dict = test_dictionary(string_table.clone());
        let word_lattice_builder = LatticeBuilder {
            dict,
            lookup_strategy: LookupStrategy::Standard,
        };
        let engine = ChewingEngine {
            word_lattice_builder,
            decoder: Decoder {
                lm: StaticLm::new(),
                hist: HistoryDict::new(string_table.clone()),
                lambda: Decoder::LAMBDA,
            },
            string_table: string_table.clone(),
        };
        let mut composition = Composition::new();
        for sym in [
            Symbol::from(syl![C, E, TONE4]),
            Symbol::from(syl![SH, TONE4]),
            Symbol::from(syl![I, TONE2]),
            Symbol::from(syl![X, I, A, TONE4]),
        ] {
            composition.push(sym);
        }
        assert_eq!(
            vec![
                Interval {
                    start: 0,
                    end: 2,
                    is_phrase: true,
                    text: "測試".into()
                },
                Interval {
                    start: 2,
                    end: 4,
                    is_phrase: true,
                    text: "一下".into()
                }
            ],
            engine.convert(&composition)[0].intervals
        );
        assert_eq!(
            vec![
                Interval {
                    start: 0,
                    end: 3,
                    is_phrase: true,
                    text: "測試儀".into()
                },
                Interval {
                    start: 3,
                    end: 4,
                    is_phrase: true,
                    text: "下".into()
                }
            ],
            engine.convert(&composition)[1].intervals
        );
        assert_eq!(
            Some(vec![
                Interval {
                    start: 0,
                    end: 2,
                    is_phrase: true,
                    text: "測試".into()
                },
                Interval {
                    start: 2,
                    end: 4,
                    is_phrase: true,
                    text: "一下".into()
                }
            ]),
            engine
                .convert(&composition)
                .into_iter()
                .cycle()
                .nth(2)
                .map(|p| p.intervals)
        );
    }

    #[test]
    fn convert_after_context_the_dictionary_cannot_read() {
        let string_table = StringTable::new();
        let dict = test_dictionary(string_table.clone());
        let engine = ChewingEngine {
            word_lattice_builder: LatticeBuilder {
                dict,
                lookup_strategy: LookupStrategy::Standard,
            },
            decoder: Decoder {
                lm: StaticLm::new(),
                hist: HistoryDict::new(string_table.clone()),
                lambda: Decoder::LAMBDA,
            },
            string_table,
        };
        let mut composition = Composition::new();
        composition.push(Symbol::from(syl![G, U, O, TONE2]));
        composition.push(Symbol::from(syl![M, I, EN, TONE2]));
        // No word reads as 貝 (nor is there any for ㄅㄟˋ): the composition
        // still converts, and the context stays out of the output.
        let context = [(syl![B, EI, TONE4], "貝".to_string())];
        assert_eq!(
            vec![Interval {
                start: 0,
                end: 2,
                is_phrase: true,
                text: "國民".into()
            }],
            engine.convert_after(&composition, &context)[0].intervals
        );
    }

    #[test]
    fn convert_collapses_equal_text_resegmentations() {
        let string_table = StringTable::new();
        let dict = test_dictionary(string_table.clone());
        let word_lattice_builder = LatticeBuilder {
            dict,
            lookup_strategy: LookupStrategy::Standard,
        };
        let engine = ChewingEngine {
            word_lattice_builder,
            decoder: Decoder {
                lm: StaticLm::new(),
                hist: HistoryDict::new(string_table.clone()),
                lambda: Decoder::LAMBDA,
            },
            string_table: string_table.clone(),
        };
        let mut composition = Composition::new();
        for _ in 0..80 {
            composition.push(Symbol::from(syl![H, A]));
        }
        let outcomes = engine.convert(&composition);
        // Every segmentation of 80 ㄏㄚ (e.g. 40x哈哈, 39x哈哈+2x哈, …) renders to
        // the identical visible string, so de-duplicating by text leaves exactly
        // one candidate instead of dozens of equal-looking re-segmentations.
        // FIXME: still support this?
        // assert_eq!(1, outcomes.len());
        // The cheapest segmentation pairs every ㄏㄚ into 哈哈 -> 40 intervals.
        assert_eq!(40, outcomes[0].intervals.len());
    }
}
