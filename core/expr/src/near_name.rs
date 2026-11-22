// SPDX-License-Identifier: MIT
//! The near miss of an unknown name (mitcad#2181): a name typed with a
//! letter wrong, missing, extra or two letters swapped ("table_lenght" for
//! "table_length"), or in other case ("Width" for "width"), so that a
//! message can ask "did you mean table_length?".

/// The candidate closest to `name`, when one is near enough: the fewest
/// edits (a letter changed, added or left out, two neighbours swapped;
/// letters compared without case, a difference only in case counting as
/// none), at most one edit for a name of 3 to 5 letters and two for a
/// longer one; none for a name of one or two letters, where nearly every
/// name is near. Of candidates equally near, the first. `name` itself is
/// never its own near miss.
pub fn near_name<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let typed: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
    let allowed = match name.chars().count() {
        0..=2 => return None,
        3..=5 => 1,
        _ => 2,
    };
    let mut best: Option<(&'a str, usize)> = None;
    for candidate in candidates {
        if candidate == name {
            continue;
        }
        let other: Vec<char> = candidate.chars().flat_map(char::to_lowercase).collect();
        if typed.len().abs_diff(other.len()) > allowed {
            continue;
        }
        let distance = edits(&typed, &other);
        if distance <= allowed && best.is_none_or(|(_, nearest)| distance < nearest) {
            best = Some((candidate, distance));
        }
    }
    best.map(|(candidate, _)| candidate)
}

/// The optimal string alignment distance of `a` and `b`: changes,
/// additions, removals and swaps of two neighbours.
fn edits(a: &[char], b: &[char]) -> usize {
    let width = b.len() + 1;
    let mut rows = vec![0usize; (a.len() + 1) * width];
    let at = |i: usize, j: usize| i * width + j;
    for i in 0..=a.len() {
        rows[at(i, 0)] = i;
    }
    for j in 0..=b.len() {
        rows[at(0, j)] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (rows[at(i - 1, j)] + 1)
                .min(rows[at(i, j - 1)] + 1)
                .min(rows[at(i - 1, j - 1)] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(rows[at(i - 2, j - 2)] + 1);
            }
            rows[at(i, j)] = best;
        }
    }
    rows[at(a.len(), b.len())]
}

#[cfg(test)]
mod tests {
    use super::near_name;

    #[test]
    fn a_swapped_pair_a_missing_and_an_extra_letter_are_near() {
        let names = ["sheet_thickness", "table_length", "width"];
        assert_eq!(near_name("table_lenght", names), Some("table_length"));
        assert_eq!(near_name("table_lngth", names), Some("table_length"));
        assert_eq!(
            near_name("sheet_thicknesss", names),
            Some("sheet_thickness")
        );
        assert_eq!(near_name("Width", names), Some("width"));
        assert_eq!(near_name("widht", names), Some("width"));
    }

    #[test]
    fn far_names_short_names_and_the_name_itself_are_not() {
        let names = ["table_length", "width", "d1"];
        assert_eq!(near_name("depth", names), None);
        assert_eq!(near_name("d2", names), None);
        assert_eq!(near_name("width", names), None);
        assert_eq!(near_name("tab_len", names), None);
    }

    #[test]
    fn the_nearest_wins_and_the_first_of_equals() {
        assert_eq!(near_name("lenght", ["length2", "length"]), Some("length"));
        assert_eq!(near_name("wdth", ["width", "wdth2"]), Some("width"));
    }
}
