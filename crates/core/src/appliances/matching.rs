//! Which model family a make and model look like. Conservative by design
//! (P2, D-29): it only suggests, always says why, and the person confirms.
//!
//! 1. **Brand.** The make, as uppercase letters and digits, must equal a
//!    brand's name or one of its label spellings (`aliases`); a make that
//!    merely *starts* with a spelling of four or more characters ("Whirlpool
//!    Corp") counts, but only as a possible match. No make, no match: a
//!    model number alone is never enough.
//! 2. **Model.** The model number is tested against the model patterns of
//!    that brand's families (`#` digit, `@` letter, `?` either, `*` any run),
//!    narrowed to the kind of thing when it is known. A read label often
//!    confuses 0 and O, 1 and I, 5 and S, 8 and B, 2 and Z: up to two such
//!    swaps are tried, and a match that needed one is only "possible", with
//!    the corrected reading shown.
//! 3. **Ranking.** The pattern with the most fixed characters wins ("RA16AY*"
//!    over "RA##@@*"). A match is *strong* only when the brand is exact, no
//!    swap was needed, the family's patterns are confirmed, and no other
//!    family fits as well. Anything else is shown as possible, and when the
//!    brand matches but no pattern does, the brand's families are offered by
//!    name, with "none of these".

use serde::Serialize;

use super::{Library, normalise_brand, normalise_model};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Brand exact, model fits one family's confirmed pattern.
    Model,
    /// Fits, with a caveat (a read character swapped, a tie, an unconfirmed
    /// pattern, a loose brand match).
    Possible,
    /// Same brand; the model number did not fit any pattern.
    Brand,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FamilyMatch {
    /// `<brand>/<family>`.
    pub family: String,
    pub confidence: Confidence,
    /// Plain sentences: why it matched.
    pub reasons: Vec<String>,
    /// The pattern that fitted.
    pub pattern: Option<String>,
    /// When characters were swapped to fit: the model as it then reads.
    pub corrected_model: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MatchResult {
    /// Brand ids whose name or label spelling matched the make.
    pub brands: Vec<String>,
    /// Best first. Empty when nothing fits.
    pub families: Vec<FamilyMatch>,
}

/// Models shorter than this are not matched against patterns.
const MIN_MODEL: usize = 4;
/// At most this many swapped characters.
const MAX_SWAPS: usize = 2;
/// Up to this many suggestions.
const MAX_RESULTS: usize = 5;
/// Up to this many of a brand's families when no pattern fits.
const MAX_BRAND_LIST: usize = 12;

/// Letters a camera reads for a digit, and the digit they stand for.
fn as_digit(c: char) -> Option<char> {
    match c {
        'O' | 'Q' | 'D' => Some('0'),
        'I' | 'L' => Some('1'),
        'Z' => Some('2'),
        'S' => Some('5'),
        'B' => Some('8'),
        _ => None,
    }
}

/// Digits a camera reads for a letter, and the letter they stand for.
fn as_letter(c: char) -> Option<char> {
    match c {
        '0' => Some('O'),
        '1' => Some('I'),
        '2' => Some('Z'),
        '5' => Some('S'),
        '8' => Some('B'),
        _ => None,
    }
}

/// Could `m` (as read) be the literal `p`?
fn swap_for(p: char, m: char) -> Option<char> {
    if p.is_ascii_digit() {
        as_digit(m).filter(|d| *d == p)
    } else {
        as_letter(m).filter(|l| *l == p)
    }
}

/// Whole-string match of a normalised model against a normalised pattern,
/// allowing up to `budget` swapped characters. Returns the fewest swaps and
/// the model as read with them.
fn fit(p: &[char], m: &[char], budget: usize) -> Option<(usize, Vec<char>)> {
    let Some(&pc) = p.first() else {
        return m.is_empty().then(|| (0, Vec::new()));
    };
    if pc == '*' {
        let mut best: Option<(usize, Vec<char>)> = None;
        for i in 0..=m.len() {
            if !m[..i].iter().all(char::is_ascii_alphanumeric) {
                break;
            }
            if let Some((cost, rest)) = fit(&p[1..], &m[i..], budget)
                && best.as_ref().is_none_or(|(b, _)| cost < *b)
            {
                let mut out = m[..i].to_vec();
                out.extend(rest);
                best = Some((cost, out));
                if cost == 0 {
                    break;
                }
            }
        }
        return best;
    }
    let &mc = m.first()?;
    let (cost, used) = match pc {
        '#' if mc.is_ascii_digit() => (0, mc),
        '#' => (1, as_digit(mc)?),
        '@' if mc.is_ascii_alphabetic() => (0, mc),
        '@' => (1, as_letter(mc)?),
        '?' if mc.is_ascii_alphanumeric() => (0, mc),
        _ if pc == mc => (0, mc),
        _ => (1, swap_for(pc, mc)?),
    };
    if cost > budget {
        return None;
    }
    let (rest_cost, rest) = fit(&p[1..], &m[1..], budget - cost)?;
    let mut out = vec![used];
    out.extend(rest);
    Some((cost + rest_cost, out))
}

/// Exact whole-string match (no swaps), as the library's own checker does.
pub fn pattern_matches(pattern: &str, model: &str) -> bool {
    let p: Vec<char> = normalise_model(pattern).chars().collect();
    let m: Vec<char> = normalise_model(model).chars().collect();
    fit(&p, &m, 0).is_some()
}

/// Fixed characters in a pattern: how specific it is.
fn literals(pattern: &str) -> usize {
    normalise_model(pattern)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .count()
}

/// Brands whose name or label spelling matches the make. `true` = exact.
fn brands_for(lib: &Library, make: &str) -> Vec<(String, bool, String)> {
    let m = normalise_brand(make);
    if m.len() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (id, b) in &lib.brands {
        let names = std::iter::once(&b.name).chain(&b.aliases);
        let mut hit: Option<(bool, String)> = None;
        for n in names {
            let nn = normalise_brand(n);
            if nn == m {
                hit = Some((true, n.clone()));
                break;
            }
            if nn.len() >= 4 && m.starts_with(&nn) && hit.is_none() {
                hit = Some((false, n.clone()));
            }
        }
        if let Some((exact, spelled)) = hit {
            out.push((id.clone(), exact, spelled));
        }
    }
    // Exact matches only, when there are any.
    if out.iter().any(|(_, e, _)| *e) {
        out.retain(|(_, e, _)| *e);
    }
    out
}

/// Suggest families for a make and model. `template`: the kind of thing it
/// is, when known (only families extending that template are offered).
pub fn find(lib: &Library, make: &str, model: &str, template: Option<&str>) -> MatchResult {
    let brands = brands_for(lib, make);
    if brands.is_empty() {
        return MatchResult::default();
    }
    let model_n: Vec<char> = normalise_model(model).chars().collect();
    let model_ok = model_n.len() >= MIN_MODEL
        && model_n.len() <= 40
        && model_n.iter().all(char::is_ascii_alphanumeric);

    struct Cand {
        id: String,
        swaps: usize,
        lits: usize,
        pattern: String,
        corrected: String,
        brand_exact: bool,
        brand_text: String,
        unverified: bool,
    }
    let mut cands: Vec<Cand> = Vec::new();
    let mut brand_only: Vec<(String, bool, String)> = Vec::new();
    for (bid, exact, spelled) in &brands {
        for (fid, f) in lib.families.iter().filter(|(_, f)| &f.brand == bid) {
            if let Some(t) = template
                && lib.template_of(f).as_deref() != Some(t)
            {
                continue;
            }
            let mut best: Option<(usize, usize, String, String)> = None;
            if model_ok {
                for pat in &f.model_patterns {
                    let p: Vec<char> = normalise_model(pat).chars().collect();
                    if let Some((swaps, read)) = fit(&p, &model_n, MAX_SWAPS) {
                        let lits = literals(pat);
                        let better = best.as_ref().is_none_or(|(s, l, _, _)| {
                            (swaps, std::cmp::Reverse(lits)) < (*s, std::cmp::Reverse(*l))
                        });
                        if better {
                            best = Some((swaps, lits, pat.clone(), read.into_iter().collect()));
                        }
                    }
                }
            }
            match best {
                Some((swaps, lits, pattern, corrected)) => cands.push(Cand {
                    id: fid.clone(),
                    swaps,
                    lits,
                    pattern,
                    corrected,
                    brand_exact: *exact,
                    brand_text: spelled.clone(),
                    unverified: f.unverified,
                }),
                None => brand_only.push((fid.clone(), *exact, spelled.clone())),
            }
        }
    }

    let brand_reason = |exact: bool, spelled: &str, brand: &str| {
        if exact {
            format!(
                "The make “{}” is how {brand} is written on its labels.",
                make.trim()
            )
        } else {
            format!(
                "The make “{}” starts with “{spelled}”, a name {brand} uses.",
                make.trim()
            )
        }
    };
    let brand_name = |fid: &str| {
        let bid = fid.split('/').next().unwrap_or_default();
        lib.brands
            .get(bid)
            .map(|b| b.name.clone())
            .unwrap_or_default()
    };

    cands.sort_by(|a, b| {
        (a.swaps, std::cmp::Reverse(a.lits), &a.id).cmp(&(
            b.swaps,
            std::cmp::Reverse(b.lits),
            &b.id,
        ))
    });
    let mut families = Vec::new();
    let top_tied =
        cands.len() > 1 && cands[0].swaps == cands[1].swaps && cands[0].lits == cands[1].lits;
    for (i, c) in cands.iter().enumerate() {
        let mut reasons = vec![brand_reason(
            c.brand_exact,
            &c.brand_text,
            &brand_name(&c.id),
        )];
        let model_shown: String = model.trim().to_string();
        if c.swaps == 0 {
            reasons.push(format!(
                "Model {model_shown} fits this family’s model numbers ({}).",
                c.pattern
            ));
        } else {
            reasons.push(format!(
                "Model {model_shown} fits this family’s model numbers ({}) if it reads {}: labels are hard to read, and 0 and O look alike.",
                c.pattern, c.corrected
            ));
        }
        if c.unverified {
            reasons.push("The library has not confirmed this family’s model numbers.".into());
        }
        let strong = i == 0 && !top_tied && c.swaps == 0 && c.brand_exact && !c.unverified;
        if i == 0 && top_tied {
            reasons.push("More than one family fits as well; check which is yours.".into());
        } else if i > 0 {
            reasons.push("A closer match is listed first.".into());
        }
        families.push(FamilyMatch {
            family: c.id.clone(),
            confidence: if strong {
                Confidence::Model
            } else {
                Confidence::Possible
            },
            reasons,
            pattern: Some(c.pattern.clone()),
            corrected_model: (c.swaps > 0).then(|| c.corrected.clone()),
        });
    }
    if families.is_empty() {
        for (fid, exact, spelled) in &brand_only {
            let mut reasons = vec![brand_reason(*exact, spelled, &brand_name(fid))];
            reasons.push(if model_ok {
                "The model number does not fit a model pattern the library knows for this brand."
                    .into()
            } else {
                "No model number to compare yet.".into()
            });
            families.push(FamilyMatch {
                family: fid.clone(),
                confidence: Confidence::Brand,
                reasons,
                pattern: None,
                corrected_model: None,
            });
        }
    }
    // Pattern matches: the best few. Brand only: the brand's families in
    // this kind, by name, so the person can pick (or say none of these).
    if families.iter().any(|f| f.confidence != Confidence::Brand) {
        families.truncate(MAX_RESULTS);
    } else {
        families.truncate(MAX_BRAND_LIST);
    }
    MatchResult {
        brands: brands.into_iter().map(|(b, _, _)| b).collect(),
        families,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        normalise_model(s).chars().collect()
    }

    #[test]
    fn exact_patterns() {
        assert!(pattern_matches("WRF5##*", "wrf555sdfz"));
        assert!(pattern_matches("XG##T##HE##U#", "XG50T12HE40U0"));
        assert!(pattern_matches("PROG##-##N RH67", "PROG50-38N RH67"));
        assert!(!pattern_matches("WRF5##", "WRF555SDFZ"));
        assert!(!pattern_matches("XG@#", "XG55"));
    }

    #[test]
    fn swaps_are_counted_and_corrected() {
        // "XG5O..." read with the letter O where a digit belongs.
        let (n, read) = fit(&chars("XG##T##HE##U#"), &chars("XG5OT12HE40U0"), 2).unwrap();
        assert_eq!(n, 1);
        assert_eq!(read.into_iter().collect::<String>(), "XG50T12HE40U0");
        // A literal letter read as a digit: "WRF" read "WRF" fine, "0" for "O".
        let (n, read) = fit(&chars("GDT6##*"), &chars("GDT6950"), 2).unwrap();
        assert_eq!(n, 0);
        assert_eq!(read.len(), 7);
        let (n, _) = fit(&chars("WD@@####"), &chars("WD0E1234"), 2).unwrap();
        assert_eq!(n, 1);
        // Too many swaps.
        assert!(fit(&chars("XG##T##"), &chars("XGOOTII"), 2).is_none());
        // Not confusable at all.
        assert!(fit(&chars("XG##"), &chars("XGAA"), 2).is_none());
    }
}
